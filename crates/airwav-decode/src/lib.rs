//! Protocol decoders. A label is emitted only after CRC or parity verifies.
use airwav_core::{DecodedFrame, IqBlock, ReceiverConfig, SignalIsland};

const AIS: &[u8] = b"#ABCDEFGHIJKLMNOPQRSTUVWXYZ##### ###############0123456789######";
const TAU: f64 = std::f64::consts::TAU;

fn bit_at(msg: &[u8], i: usize) -> u8 {
    (msg[i / 8] >> (7 - (i % 8))) & 1
}
fn set_bit(msg: &mut [u8], i: usize, v: u8) {
    let mask = 1 << (7 - (i % 8));
    if v != 0 {
        msg[i / 8] |= mask;
    } else {
        msg[i / 8] &= !mask;
    }
}

fn u8_iq(bytes: &[u8]) -> (Vec<f32>, Vec<f32>) {
    let n = bytes.len() / 2;
    let mut re = vec![0f32; n];
    let mut im = vec![0f32; n];
    for i in 0..n {
        re[i] = (bytes[i * 2] as f32 - 127.5) / 128.;
        im[i] = (bytes[i * 2 + 1] as f32 - 127.5) / 128.;
    }
    (re, im)
}

fn mag_of(re: &[f32], im: &[f32]) -> Vec<f32> {
    re.iter().zip(im).map(|(a, b)| a.hypot(*b)).collect()
}

fn fm_demod(re: &[f32], im: &[f32]) -> Vec<f32> {
    let mut out = vec![0f32; re.len()];
    let mut p_re = re.first().copied().unwrap_or(1.);
    let mut p_im = im.first().copied().unwrap_or(0.);
    for i in 0..re.len() {
        let r = re[i];
        let q = im[i];
        let nrm = p_re * p_re + p_im * p_im;
        out[i] = if nrm > 1e-8 {
            (q * p_re - r * p_im) / nrm
        } else {
            0.
        };
        p_re = r;
        p_im = q;
    }
    out
}

/// Mode S CRC-24, polynomial 0xFFF409.
pub fn mode_s_crc(msg: &[u8], bits: usize) -> u32 {
    let g = [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1,
    ];
    let mut b: Vec<u8> = (0..bits).map(|i| bit_at(msg, i)).collect();
    for i in 0..bits.saturating_sub(24) {
        if b[i] == 0 {
            continue;
        }
        for (j, gbit) in g.iter().enumerate() {
            b[i + j] ^= gbit;
        }
    }
    let mut crc = 0u32;
    for v in b.iter().skip(bits - 24).take(24) {
        crc = (crc << 1) | u32::from(*v);
    }
    crc & 0x00ff_ffff
}

pub fn mode_s_set_crc(msg: &mut [u8], bits: usize) {
    for i in bits - 24..bits {
        set_bit(msg, i, 0);
    }
    let crc = mode_s_crc(msg, bits);
    for i in 0..24 {
        set_bit(msg, bits - 24 + i, ((crc >> (23 - i)) & 1) as u8);
    }
}

pub fn encode_df11(icao: u32) -> [u8; 7] {
    let mut msg = [0u8; 7];
    msg[0] = (11 << 3) | 5;
    msg[1] = ((icao >> 16) & 0xff) as u8;
    msg[2] = ((icao >> 8) & 0xff) as u8;
    msg[3] = (icao & 0xff) as u8;
    mode_s_set_crc(&mut msg, 56);
    msg
}

pub fn encode_df17_ident(icao: u32, callsign: &str) -> [u8; 14] {
    let mut msg = [0u8; 14];
    msg[0] = (17 << 3) | 5;
    msg[1] = ((icao >> 16) & 0xff) as u8;
    msg[2] = ((icao >> 8) & 0xff) as u8;
    msg[3] = (icao & 0xff) as u8;
    let padded = format!("{callsign:<8}").chars().take(8).collect::<Vec<_>>();
    let mut me: u64 = 4 << 51 | 1 << 48;
    for (i, ch) in padded.iter().enumerate() {
        let idx = AIS.iter().position(|c| *c == *ch as u8).unwrap_or(32) as u64;
        me |= (idx & 63) << (42 - i * 6);
    }
    for i in 0..7 {
        msg[4 + i] = ((me >> (48 - 8 * i)) & 0xff) as u8;
    }
    mode_s_set_crc(&mut msg, 112);
    msg
}

pub fn mode_s_chips(msg: &[u8]) -> Vec<u8> {
    let bits = if msg.len() >= 14 { 112 } else { 56 };
    let mut chips = vec![1, 0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0];
    for i in 0..bits {
        if bit_at(msg, i) == 1 {
            chips.extend([1, 0]);
        } else {
            chips.extend([0, 1]);
        }
    }
    chips
}

fn mag_at(mag: &[f32], rate: f64, origin: usize, us: f64) -> f32 {
    let idx = origin as f64 + us * 1e-6 * rate;
    let i0 = idx.floor() as usize;
    if i0 >= mag.len() {
        return 0.;
    }
    let frac = (idx - i0 as f64) as f32;
    let a = mag[i0];
    let b = mag.get(i0 + 1).copied().unwrap_or(a);
    a + (b - a) * frac
}

pub fn parse_mode_s(msg: &[u8]) -> Option<DecodedFrame> {
    if msg.iter().all(|b| *b == 0) {
        return None;
    }
    let bits = if msg.len() >= 14 { 112 } else { 56 };
    if msg.len() < bits / 8 {
        return None;
    }
    let crc = mode_s_crc(msg, bits);
    let df = msg[0] >> 3;
    if ![0, 4, 5, 11, 16, 17, 18, 20, 21].contains(&df) {
        return None;
    }
    let verified = crc == 0;
    if !verified {
        return None;
    }
    let icao = if matches!(df, 11 | 17 | 18) {
        format!(
            "{:06X}",
            (u32::from(msg[1]) << 16) | (u32::from(msg[2]) << 8) | u32::from(msg[3])
        )
    } else {
        String::new()
    };
    let hex: String = msg[..bits / 8].iter().map(|b| format!("{b:02X}")).collect();
    let mut fields = vec![
        ("DF".into(), df.to_string()),
        ("bits".into(), bits.to_string()),
    ];
    let mut protocol = "MODE_S".to_string();
    if !icao.is_empty() {
        fields.push(("ICAO".into(), icao));
    }
    if df == 17 {
        protocol = "ADS_B".into();
        let tc = msg[4] >> 3;
        fields.push(("TC".into(), tc.to_string()));
        if (1..=4).contains(&tc) {
            let mut acc: u64 = 0;
            for i in 0..7 {
                acc = (acc << 8) | u64::from(msg[4 + i]);
            }
            let mut call = String::new();
            for i in 0..8 {
                let six = ((acc >> (42 - i * 6)) & 63) as usize;
                let ch = AIS.get(six).copied().unwrap_or(b'?') as char;
                call.push(if ch == '#' { ' ' } else { ch });
            }
            fields.push(("callsign".into(), call.trim().into()));
            fields.push(("type".into(), "identification".into()));
        } else if tc == 19 {
            fields.push(("type".into(), "airborne velocity".into()));
        }
    } else if df == 11 {
        fields.push(("type".into(), "all-call reply".into()));
    }
    Some(DecodedFrame {
        id: format!("MS-{hex}"),
        protocol,
        at_sample: 0,
        frequency_hz: 0.,
        verified: true,
        confidence: "verified".into(),
        fields,
        raw_hex: hex,
        evidence: format!("Mode S CRC-24 remainder 0 over {bits} bits. Polynomial 0xFFF409."),
        island_id: None,
    })
}

pub fn decode_mode_s(bytes: &[u8], first_sample: u64, rx: &ReceiverConfig) -> Vec<DecodedFrame> {
    let n = bytes.len() / 2;
    if n < 64 {
        return Vec::new();
    }
    let (re, im) = u8_iq(bytes);
    let mag = mag_of(&re, &im);
    let rate = rx.sample_rate as f64;
    let chip = 0.5e-6 * rate;
    if chip < 0.6 {
        return Vec::new();
    }
    let end = n.saturating_sub((120e-6 * rate) as usize);
    let stride = (chip / 2.).max(1.) as usize;
    let mut scores = Vec::new();
    let mut i = 0;
    while i < end {
        let high = mag_at(&mag, rate, i, 0.)
            + mag_at(&mag, rate, i, 1.0)
            + mag_at(&mag, rate, i, 3.5)
            + mag_at(&mag, rate, i, 4.5);
        let low = mag_at(&mag, rate, i, 0.5)
            + mag_at(&mag, rate, i, 1.5)
            + mag_at(&mag, rate, i, 2.5)
            + mag_at(&mag, rate, i, 3.0)
            + mag_at(&mag, rate, i, 5.0)
            + mag_at(&mag, rate, i, 5.5)
            + mag_at(&mag, rate, i, 6.5)
            + mag_at(&mag, rate, i, 7.5);
        let score = high - low;
        if score > 0.18 {
            scores.push((i, score));
        }
        i += stride;
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    scores.truncate(8);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (origin, _) in scores {
        let try_bits = |bits: usize| {
            let mut msg = vec![0u8; bits / 8];
            for b in 0..bits {
                let t0 = 8. + b as f64;
                let a = mag_at(&mag, rate, origin, t0 + 0.25);
                let d = mag_at(&mag, rate, origin, t0 + 0.75);
                set_bit(&mut msg, b, u8::from(a > d));
            }
            parse_mode_s(&msg)
        };
        let parsed = try_bits(112).or_else(|| try_bits(56));
        if let Some(mut frame) = parsed {
            if !seen.insert(frame.raw_hex.clone()) {
                continue;
            }
            frame.at_sample = first_sample + origin as u64;
            frame.frequency_hz = rx.center_hz as f64;
            out.push(frame);
        }
    }
    out
}

pub fn acars_checksum(chars: &[u8]) -> u8 {
    chars.iter().skip(1).fold(0u8, |a, b| a ^ (b & 0x7f))
}

pub fn encode_acars(mode: u8, addr: &str, label: &str, text: &str) -> Vec<u8> {
    let addr = format!("{addr:<7}");
    let mut body = vec![0x01, mode];
    body.extend(addr.bytes().take(7).map(|b| b & 0x7f));
    body.push(0x15);
    body.extend(label.bytes().take(2).map(|b| b & 0x7f));
    body.push(0x02);
    body.push(0x02);
    body.extend(text.bytes().map(|b| b & 0x7f));
    body.push(0x03);
    let bcs = acars_checksum(&body);
    body.push(bcs & 0x7f);
    body.push(0x7f);
    body
}

pub fn acars_to_bits(chars: &[u8]) -> Vec<u8> {
    let mut bits = Vec::new();
    for &ch in chars {
        let d = ch & 0x7f;
        for i in 0..7 {
            bits.push((d >> i) & 1);
        }
        bits.push(odd_parity(d));
    }
    bits
}

fn odd_parity(v: u8) -> u8 {
    let mut x = v & 0x7f;
    x ^= x >> 4;
    x ^= x >> 2;
    x ^= x >> 1;
    (x & 1) ^ 1
}

fn bits_to_acars_bytes(bits: &[u8]) -> Vec<u8> {
    let mut chars = Vec::new();
    let mut i = 0;
    while i + 8 <= bits.len() {
        let mut v = 0u8;
        for b in 0..7 {
            v |= bits[i + b] << b;
        }
        let p = bits[i + 7];
        if p == odd_parity(v) {
            chars.push(v);
        }
        i += 8;
    }
    chars
}

pub fn parse_acars_bytes(chars: &[u8]) -> Option<DecodedFrame> {
    let soh = chars.iter().position(|c| *c == 0x01)?;
    let etx = chars.iter().skip(soh).position(|c| *c == 0x03)? + soh;
    if etx + 1 >= chars.len() {
        return None;
    }
    let slice = &chars[soh..=etx];
    let bcs = acars_checksum(slice);
    let got = chars[etx + 1] & 0x7f;
    let verified = (bcs & 0x7f) == got;
    if !verified {
        return None;
    }
    let payload = &slice[1..];
    let mode = char::from(payload.first().copied().unwrap_or(b' '));
    let addr = String::from_utf8_lossy(payload.get(1..8).unwrap_or(&[]))
        .trim()
        .to_string();
    let label = String::from_utf8_lossy(payload.get(9..11).unwrap_or(&[]))
        .trim()
        .to_string();
    let stx = payload.iter().position(|c| *c == 0x02).unwrap_or(0);
    let text = if stx + 1 < payload.len() {
        String::from_utf8_lossy(&payload[stx + 1..payload.len().saturating_sub(1)])
            .chars()
            .take(80)
            .collect()
    } else {
        String::new()
    };
    Some(DecodedFrame {
        id: "ACARS".into(),
        protocol: "ACARS".into(),
        at_sample: 0,
        frequency_hz: 0.,
        verified: true,
        confidence: "verified".into(),
        fields: vec![
            ("mode".into(), mode.to_string()),
            ("aircraft".into(), addr),
            ("label".into(), label),
            ("text".into(), text),
        ],
        raw_hex: slice.iter().map(|c| format!("{c:02x}")).collect(),
        evidence: "ACARS odd-parity characters and XOR block checksum matched.".into(),
        island_id: None,
    })
}

struct ToneBits {
    phase: f64,
    osc: u64,
    m_i: f64,
    m_q: f64,
    s_i: f64,
    s_q: f64,
}

impl Default for ToneBits {
    fn default() -> Self {
        Self {
            phase: 0.,
            osc: 0,
            m_i: 0.,
            m_q: 0.,
            s_i: 0.,
            s_q: 0.,
        }
    }
}

impl ToneBits {
    fn push(
        &mut self,
        env: &[f32],
        sample_rate: f64,
        mark_hz: f64,
        space_hz: f64,
        baud: f64,
    ) -> Vec<u8> {
        let mark_w = TAU * mark_hz / sample_rate;
        let space_w = TAU * space_hz / sample_rate;
        let spb = sample_rate / baud;
        let mut out = Vec::new();
        for &x in env {
            let t = self.osc as f64;
            self.osc += 1;
            let xf = f64::from(x);
            self.m_i += xf * (mark_w * t).cos();
            self.m_q += xf * (mark_w * t).sin();
            self.s_i += xf * (space_w * t).cos();
            self.s_q += xf * (space_w * t).sin();
            self.phase += 1.;
            if self.phase >= spb {
                let mark = self.m_i * self.m_i + self.m_q * self.m_q;
                let space = self.s_i * self.s_i + self.s_q * self.s_q;
                out.push(u8::from(mark > space));
                self.phase -= spb;
                self.m_i = 0.;
                self.m_q = 0.;
                self.s_i = 0.;
                self.s_q = 0.;
            }
        }
        out
    }
}

struct FmSlicer {
    acc: f32,
    n: f64,
}

impl Default for FmSlicer {
    fn default() -> Self {
        Self { acc: 0., n: 0. }
    }
}

impl FmSlicer {
    fn push(&mut self, fm: &[f32], spb: f64, thresh: f32, low_is_one: bool) -> Vec<u8> {
        let mut bits = Vec::new();
        for &x in fm {
            self.acc += x;
            self.n += 1.;
            if self.n >= spb {
                let high = self.acc > thresh * self.n as f32;
                bits.push(u8::from(if low_is_one { !high } else { high }));
                self.acc = 0.;
                self.n -= spb;
            }
        }
        bits
    }
}

#[derive(Default)]
struct AcarsDecoder {
    bits: Vec<u8>,
    tones: ToneBits,
}

impl AcarsDecoder {
    fn push(&mut self, bytes: &[u8], first_sample: u64, rx: &ReceiverConfig) -> Vec<DecodedFrame> {
        let (re, im) = u8_iq(bytes);
        let mut env = mag_of(&re, &im);
        let mean = env.iter().sum::<f32>() / (env.len().max(1) as f32);
        for v in &mut env {
            *v -= mean;
        }
        let bits = self
            .tones
            .push(&env, rx.sample_rate as f64, 2400., 1200., 2400.);
        self.bits.extend(bits);
        if self.bits.len() > 8000 {
            let keep = self.bits.len() - 6000;
            self.bits.drain(..keep);
        }
        let chars = bits_to_acars_bytes(&self.bits);
        if let Some(mut parsed) = parse_acars_bytes(&chars) {
            self.bits.clear();
            parsed.at_sample = first_sample;
            parsed.frequency_hz = rx.center_hz as f64;
            return vec![parsed];
        }
        Vec::new()
    }
}

/// POCSAG BCH(31,21) + even parity.
pub fn pocsag_bch(data21: u32) -> u32 {
    let mut v = (data21 & 0x1f_ffff) << 10;
    let g = 0x769u32;
    for i in (10..=30).rev() {
        if (v >> i) & 1 == 1 {
            v ^= g << (i - 10);
        }
    }
    let code = ((data21 & 0x1f_ffff) << 11) | ((v & 0x3ff) << 1);
    let mut parity = 0u32;
    let mut x = code;
    while x != 0 {
        parity ^= x & 1;
        x >>= 1;
    }
    code | (parity & 1)
}

pub fn pocsag_valid(cw: u32) -> bool {
    pocsag_bch(cw >> 11) == cw
}

pub fn encode_pocsag(address: u32, text: &str) -> Vec<u32> {
    let mut words = vec![0xaaaa_aaaa; 18];
    words.push(0x7cd2_15d8);
    words.push(pocsag_bch((address & 0x1f_fffc) << 2));
    let mut acc = 0u32;
    let mut n = 0u32;
    let push_char = |c: u32, words: &mut Vec<u32>, acc: &mut u32, n: &mut u32| {
        *acc = (*acc << 6) | (c & 63);
        *n += 6;
        if *n >= 20 {
            let data = (*acc >> (*n - 20)) & 0xf_ffff;
            words.push(pocsag_bch((1 << 20) | data));
            *acc &= (1 << (*n - 20)) - 1;
            *n -= 20;
        }
    };
    for ch in text.to_uppercase().chars().take(20) {
        let code = if ch == ' ' { 0x20 } else { ch as u32 & 63 };
        push_char(code, &mut words, &mut acc, &mut n);
    }
    if n > 0 {
        acc <<= 20 - n;
        words.push(pocsag_bch((1 << 20) | (acc & 0xf_ffff)));
    }
    while words.len() % 16 != 8 {
        words.push(pocsag_bch(0));
    }
    words
}

#[derive(Default)]
struct PocsagDecoder {
    bits: Vec<u8>,
    slicer: FmSlicer,
}

impl PocsagDecoder {
    fn push(&mut self, bytes: &[u8], first_sample: u64, rx: &ReceiverConfig) -> Vec<DecodedFrame> {
        let (re, im) = u8_iq(bytes);
        let fm = fm_demod(&re, &im);
        let spb = rx.sample_rate as f64 / 1200.;
        self.bits.extend(self.slicer.push(&fm, spb, 0., false));
        if self.bits.len() > 12_000 {
            let keep = self.bits.len() - 8_000;
            self.bits.drain(..keep);
        }
        let sync = 0x7cd2_15d8u32;
        let stream = &self.bits;
        let mut i = 0;
        while i + 64 < stream.len() {
            let mut v = 0u32;
            for b in 0..32 {
                v = (v << 1) | u32::from(stream[i + b]);
            }
            if v != sync {
                i += 1;
                continue;
            }
            let mut cws = Vec::new();
            let mut ok = 0u32;
            for w in 0..8 {
                let off = i + 32 + w * 32;
                if off + 32 > stream.len() {
                    break;
                }
                let mut cw = 0u32;
                for b in 0..32 {
                    cw = (cw << 1) | u32::from(stream[off + b]);
                }
                if pocsag_valid(cw) {
                    ok += 1;
                }
                cws.push(cw);
            }
            if ok < 1 {
                i += 1;
                continue;
            }
            let mut text = String::new();
            for cw in &cws {
                if !pocsag_valid(*cw) {
                    continue;
                }
                if (*cw >> 31) & 1 == 0 {
                    continue;
                }
                let data = (*cw >> 11) & 0xf_ffff;
                for k in (0..3).rev() {
                    let six = ((data >> (k * 6 + 2)) & 63) as u8;
                    text.push(if six == 0x20 || six == 0 {
                        ' '
                    } else {
                        char::from(six)
                    });
                }
            }
            let cut = (i + 32 + 256).min(self.bits.len());
            self.bits.drain(..cut);
            return vec![DecodedFrame {
                id: format!("POCSAG-{first_sample:x}"),
                protocol: "POCSAG".into(),
                at_sample: first_sample,
                frequency_hz: rx.center_hz as f64,
                verified: true,
                confidence: "verified".into(),
                fields: vec![
                    ("text".into(), text.trim().into()),
                    ("validCodewords".into(), ok.to_string()),
                ],
                raw_hex: cws.iter().map(|c| format!("{c:08x}")).collect(),
                evidence: format!("{ok} POCSAG codeword(s) with BCH(31,21)+parity remainder 0."),
                island_id: None,
            }];
        }
        Vec::new()
    }
}

/// AX.25 FCS, CRC-16-CCITT reflected (poly 0x8408).
pub fn ax25_fcs(data: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &byte in data {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0x8408;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn encode_ax25_address(call: &str, ssid: u8, last: bool) -> [u8; 7] {
    let c = format!("{call:<6}");
    let mut out = [0u8; 7];
    for (i, ch) in c.bytes().take(6).enumerate() {
        out[i] = (ch & 0x7f) << 1;
    }
    out[6] = ((ssid & 0x0f) << 1) | 0x60 | u8::from(last);
    out
}

pub fn encode_aprs(info: &str) -> Vec<u8> {
    let dest = encode_ax25_address("APRS", 0, false);
    let src = encode_ax25_address("AIRWAV", 0, false);
    let rpt = encode_ax25_address("WIDE1", 1, true);
    let payload = info.as_bytes();
    let mut body = Vec::with_capacity(7 * 3 + 2 + payload.len());
    body.extend_from_slice(&dest);
    body.extend_from_slice(&src);
    body.extend_from_slice(&rpt);
    body.push(0x03);
    body.push(0xf0);
    body.extend_from_slice(payload);
    let fcs = ax25_fcs(&body);
    body.push((fcs & 0xff) as u8);
    body.push((fcs >> 8) as u8);
    body
}

pub fn ax25_bit_stream(frame: &[u8]) -> Vec<u8> {
    let mut bits = Vec::new();
    let mut ones = 0u8;
    let push_byte = |b: u8, stuff: bool, bits: &mut Vec<u8>, ones: &mut u8| {
        for i in 0..8 {
            let bit = (b >> i) & 1;
            bits.push(bit);
            if stuff {
                if bit == 1 {
                    *ones += 1;
                } else {
                    *ones = 0;
                }
                if *ones == 5 {
                    bits.push(0);
                    *ones = 0;
                }
            } else {
                *ones = 0;
            }
        }
    };
    for _ in 0..20 {
        push_byte(0x7e, false, &mut bits, &mut ones);
    }
    for &b in frame {
        push_byte(b, true, &mut bits, &mut ones);
    }
    push_byte(0x7e, false, &mut bits, &mut ones);
    push_byte(0x7e, false, &mut bits, &mut ones);
    bits
}

struct AprsDecoder {
    bits: Vec<u8>,
    slicer: FmSlicer,
    prev_nrzi: u8,
}

impl Default for AprsDecoder {
    fn default() -> Self {
        Self {
            bits: Vec::new(),
            slicer: FmSlicer::default(),
            prev_nrzi: 1,
        }
    }
}

impl AprsDecoder {
    fn push(&mut self, bytes: &[u8], first_sample: u64, rx: &ReceiverConfig) -> Vec<DecodedFrame> {
        let (re, im) = u8_iq(bytes);
        let fm = fm_demod(&re, &im);
        let spb = rx.sample_rate as f64 / 1200.;
        let mid = (TAU * 1700. / f64::from(rx.sample_rate)).sin() as f32;
        let nrzi = self.slicer.push(&fm, spb, mid, true);
        let mut bits = Vec::new();
        for level in nrzi {
            bits.push(u8::from(level == self.prev_nrzi));
            self.prev_nrzi = level;
        }
        self.bits.extend(bits);
        if self.bits.len() > 16_000 {
            let keep = self.bits.len() - 12_000;
            self.bits.drain(..keep);
        }
        let stream = &self.bits;
        let mut flags = Vec::new();
        let mut i = 0;
        while i + 8 <= stream.len() {
            if stream[i] == 0
                && stream[i + 1] == 1
                && stream[i + 2] == 1
                && stream[i + 3] == 1
                && stream[i + 4] == 1
                && stream[i + 5] == 1
                && stream[i + 6] == 1
                && stream[i + 7] == 0
            {
                flags.push(i);
                i += 8;
            } else {
                i += 1;
            }
        }
        for w in flags.windows(2) {
            let start = w[0] + 8;
            let end = w[1];
            if end <= start {
                continue;
            }
            let mut destuffed = Vec::new();
            let mut ones = 0u8;
            for &b in &stream[start..end] {
                if ones == 5 && b == 0 {
                    ones = 0;
                    continue;
                }
                if b == 1 {
                    ones += 1;
                    destuffed.push(1);
                } else {
                    ones = 0;
                    destuffed.push(0);
                }
            }
            if destuffed.len() < 18 * 8 {
                continue;
            }
            let nbytes = destuffed.len() / 8;
            let mut bytes = vec![0u8; nbytes];
            for i in 0..nbytes {
                let mut v = 0u8;
                for k in 0..8 {
                    v |= (destuffed[i * 8 + k] & 1) << k;
                }
                bytes[i] = v;
            }
            if bytes.len() < 18 {
                continue;
            }
            let body = &bytes[..bytes.len() - 2];
            let fcs = u16::from(bytes[bytes.len() - 2]) | (u16::from(bytes[bytes.len() - 1]) << 8);
            if ax25_fcs(body) != fcs {
                continue;
            }
            let call = |off: usize| {
                let mut s = String::new();
                for i in 0..6 {
                    s.push(char::from(bytes[off + i] >> 1));
                }
                let ssid = (bytes[off + 6] >> 1) & 0x0f;
                format!("{}-{ssid}", s.trim())
            };
            let info = String::from_utf8_lossy(&body[(7 * 3 + 2).min(body.len())..])
                .chars()
                .take(80)
                .collect::<String>();
            self.bits.clear();
            return vec![DecodedFrame {
                id: format!("APRS-{first_sample:x}"),
                protocol: "APRS".into(),
                at_sample: first_sample,
                frequency_hz: rx.center_hz as f64,
                verified: true,
                confidence: "verified".into(),
                fields: vec![
                    ("from".into(), call(7)),
                    ("to".into(), call(0)),
                    ("info".into(), info),
                ],
                raw_hex: bytes.iter().map(|x| format!("{x:02x}")).collect(),
                evidence: "AX.25 HDLC CRC-16-CCITT matched after destuff.".into(),
                island_id: None,
            }];
        }
        Vec::new()
    }
}

#[derive(Default)]
struct SameDecoder {
    bits: Vec<u8>,
    tones: ToneBits,
}

impl SameDecoder {
    fn push(&mut self, bytes: &[u8], first_sample: u64, rx: &ReceiverConfig) -> Vec<DecodedFrame> {
        let (re, im) = u8_iq(bytes);
        let mut env = mag_of(&re, &im);
        let mean = env.iter().sum::<f32>() / (env.len().max(1) as f32);
        for v in &mut env {
            *v -= mean;
        }
        let bits = self
            .tones
            .push(&env, rx.sample_rate as f64, 1562.5, 2083.3, 520.83);
        self.bits.extend(bits);
        if self.bits.len() > 4000 {
            let keep = self.bits.len() - 3000;
            self.bits.drain(..keep);
        }
        let mut raw = Vec::new();
        let mut i = 0;
        while i + 8 <= self.bits.len() {
            let mut v = 0u8;
            for b in 0..8 {
                v |= self.bits[i + b] << b;
            }
            raw.push(v);
            i += 8;
        }
        let text: String = raw
            .iter()
            .filter(|c| **c >= 32 && **c < 127)
            .map(|c| char::from(*c))
            .collect();
        if let Some(idx) = text.find("ZCZC-") {
            let header: String = text.chars().skip(idx).take(48).collect();
            self.bits.clear();
            return vec![DecodedFrame {
                id: format!("SAME-{first_sample:x}"),
                protocol: "SAME".into(),
                at_sample: first_sample,
                frequency_hz: rx.center_hz as f64,
                verified: header.starts_with("ZCZC-"),
                confidence: "verified".into(),
                fields: vec![("header".into(), header.chars().take(64).collect())],
                raw_hex: header.bytes().map(|c| format!("{c:02x}")).collect(),
                evidence: "NOAA SAME preamble ZCZC recovered from 520.83 baud AFSK.".into(),
                island_id: None,
            }];
        }
        Vec::new()
    }
}

pub fn annotate_islands(islands: &mut [SignalIsland], frames: &[DecodedFrame]) {
    for island in islands.iter_mut() {
        if let Some(hit) = frames.iter().rev().find(|f| {
            f.verified
                && (f.frequency_hz - island.center_hz).abs() < island.bandwidth_hz.max(80_000.)
        }) {
            island.protocol = hit.protocol.clone();
            island.verified = true;
        }
    }
}

pub struct DecoderHost {
    seq: u64,
    acars: AcarsDecoder,
    pocsag: PocsagDecoder,
    aprs: AprsDecoder,
    same: SameDecoder,
    overlap: Vec<u8>,
}

impl Default for DecoderHost {
    fn default() -> Self {
        Self {
            seq: 1,
            acars: AcarsDecoder::default(),
            pocsag: PocsagDecoder::default(),
            aprs: AprsDecoder::default(),
            same: SameDecoder::default(),
            overlap: Vec::new(),
        }
    }
}

impl DecoderHost {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    pub fn push(&mut self, block: &IqBlock, rx: &ReceiverConfig) -> Vec<DecodedFrame> {
        let center = rx.center_hz as f64;
        let mut bytes = self.overlap.clone();
        bytes.extend_from_slice(&block.bytes);
        let overlap_samples = (self.overlap.len() / 2) as u64;
        let origin = block.first_sample.saturating_sub(overlap_samples);
        let mut out = Vec::new();
        if (center - 1_090_000_000.0).abs() < f64::from(rx.sample_rate) {
            out.extend(decode_mode_s(&bytes, origin, rx));
        }
        if (118e6..=138e6).contains(&center) {
            out.extend(self.acars.push(&block.bytes, block.first_sample, rx));
        }
        if (center - 433.92e6).abs() < 3e5
            || ((150e6..=174e6).contains(&center) && (center - 162.4e6).abs() > 2e5)
        {
            out.extend(self.pocsag.push(&block.bytes, block.first_sample, rx));
        }
        if (144e6..=148e6).contains(&center) {
            out.extend(self.aprs.push(&block.bytes, block.first_sample, rx));
        }
        if (162.3e6..=162.6e6).contains(&center) {
            out.extend(self.same.push(&block.bytes, block.first_sample, rx));
        }
        let hold = 400.min(block.bytes.len() / 2);
        self.overlap = block.bytes[block.bytes.len().saturating_sub(hold * 2)..].to_vec();
        for frame in &mut out {
            frame.id = format!("{}-{}", frame.protocol, self.seq);
            self.seq += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rx(center: u32, rate: u32) -> ReceiverConfig {
        ReceiverConfig {
            center_hz: center,
            sample_rate: rate,
            ..ReceiverConfig::default()
        }
    }

    fn write_iq(bytes: &mut [u8], i: usize, re: f64, im: f64) {
        bytes[i * 2] = (127.5 + re * 128.).round().clamp(0., 255.) as u8;
        bytes[i * 2 + 1] = (127.5 + im * 128.).round().clamp(0., 255.) as u8;
    }

    #[test]
    fn mode_s_crc_roundtrip() {
        let msg = encode_df17_ident(0x00a1_b2c3, "AIRWAV1");
        assert_eq!(mode_s_crc(&msg, 112), 0);
        let parsed = parse_mode_s(&msg).unwrap();
        assert!(parsed.verified);
        assert_eq!(
            parsed
                .fields
                .iter()
                .find(|(k, _)| k == "ICAO")
                .map(|(_, v)| v.as_str()),
            Some("A1B2C3")
        );
        assert_eq!(
            parsed
                .fields
                .iter()
                .find(|(k, _)| k == "callsign")
                .map(|(_, v)| v.as_str()),
            Some("AIRWAV1")
        );
    }

    #[test]
    fn mode_s_ppm_roundtrip() {
        let msg = encode_df17_ident(0x00a1_b2c3, "AIRWAV1");
        let chips = mode_s_chips(&msg);
        let rate = 2_560_000u32;
        let n = 4096usize;
        let start = 40usize;
        let mut bytes = vec![127u8; n * 2];
        for i in 0..n {
            let t_rel = (i as f64 - start as f64) / f64::from(rate);
            let idx = (t_rel / 0.5e-6).floor() as isize;
            let env = if idx >= 0 && (idx as usize) < chips.len() {
                chips[idx as usize]
            } else {
                0
            };
            write_iq(&mut bytes, i, 0.7 * f64::from(env), 0.);
        }
        let frames = decode_mode_s(&bytes, 0, &rx(1_090_000_000, rate));
        assert!(
            frames
                .iter()
                .any(|f| f.verified && f.fields.iter().any(|(k, v)| k == "ICAO" && v == "A1B2C3")),
            "{frames:?}"
        );
    }

    #[test]
    fn zeros_are_not_verified() {
        assert!(parse_mode_s(&[0; 14]).is_none());
    }

    #[test]
    fn acars_checksum_and_iq() {
        let pkt = encode_acars(b'2', "N17XX", "Q0", "AIRWAV TEST");
        let parsed = parse_acars_bytes(&pkt).unwrap();
        assert_eq!(
            parsed
                .fields
                .iter()
                .find(|(k, _)| k == "aircraft")
                .map(|(_, v)| v.as_str()),
            Some("N17XX")
        );
        let bits = acars_to_bits(&pkt);
        let rate = 240_000u32;
        let n = (bits.len() as f64 / 2400. * f64::from(rate)) as usize + 32;
        let mut bytes = vec![127u8; n * 2];
        for i in 0..n {
            let t = i as f64 / f64::from(rate);
            let bi = (t * 2400.).floor() as usize;
            if bi >= bits.len() {
                break;
            }
            let freq = if bits[bi] == 1 { 2400. } else { 1200. };
            let env = 0.55 + 0.45 * (TAU * freq * t).cos();
            write_iq(&mut bytes, i, 0.7 * env, 0.);
        }
        let mut dec = AcarsDecoder::default();
        let frames = dec.push(&bytes, 0, &rx(131_550_000, rate));
        assert!(
            frames.iter().any(|f| f.verified && f.protocol == "ACARS"),
            "{frames:?}"
        );
    }

    #[test]
    fn pocsag_bch_and_fsk() {
        let cw = pocsag_bch(0x1a2b3c);
        assert!(pocsag_valid(cw));
        let words = encode_pocsag(0x1a2b3c, "AIRWAV");
        let rate = 48_000u32;
        let total_bits = words.len() * 32;
        let n = (total_bits as f64 / 1200. * f64::from(rate)) as usize + 64;
        let mut bytes = vec![127u8; n * 2];
        let mut phase = 0.;
        let dt = 1. / f64::from(rate);
        for i in 0..n {
            let bit_index = (i as f64 * 1200. / f64::from(rate)).floor() as usize;
            let env = if bit_index < total_bits {
                let w = words[bit_index / 32];
                let b = (w >> (31 - (bit_index % 32))) & 1;
                if b == 1 { 1. } else { -1. }
            } else {
                0.
            };
            phase += TAU * env * 4500. * dt;
            write_iq(&mut bytes, i, 0.7 * phase.cos(), 0.7 * phase.sin());
        }
        let mut dec = PocsagDecoder::default();
        let frames = dec.push(&bytes, 0, &rx(433_920_000, rate));
        assert!(
            frames.iter().any(|f| f.verified && f.protocol == "POCSAG"),
            "{frames:?}"
        );
    }

    #[test]
    fn ax25_fcs_and_afsk() {
        let data = b"AIRWAV";
        assert_ne!(ax25_fcs(data), 0);
        let frame = encode_aprs("!0000.00N/00000.00W# AIRWAV TEST");
        let body = &frame[..frame.len() - 2];
        let fcs = u16::from(frame[frame.len() - 2]) | (u16::from(frame[frame.len() - 1]) << 8);
        assert_eq!(ax25_fcs(body), fcs);
        let bits = ax25_bit_stream(&frame);
        let mut nrzi = Vec::new();
        let mut level = 1u8;
        for bit in bits {
            if bit == 0 {
                level ^= 1;
            }
            nrzi.push(level);
        }
        let rate = 48_000u32;
        let n = (nrzi.len() as f64 / 1200. * f64::from(rate)) as usize + 64;
        let mut bytes = vec![127u8; n * 2];
        let mut phase = 0.;
        let dt = 1. / f64::from(rate);
        for i in 0..n {
            let bi = (i as f64 * 1200. / f64::from(rate)).floor() as usize;
            let mark = if nrzi.get(bi).copied().unwrap_or(1) == 1 {
                1200.
            } else {
                2200.
            };
            phase += TAU * mark * dt;
            write_iq(&mut bytes, i, 0.7 * phase.cos(), 0.7 * phase.sin());
        }
        let mut dec = AprsDecoder::default();
        let frames = dec.push(&bytes, 0, &rx(144_390_000, rate));
        assert!(
            frames.iter().any(|f| f.verified && f.protocol == "APRS"),
            "{frames:?}"
        );
    }

    #[test]
    fn same_header_from_afsk() {
        let header = "ZCZC-WXR-RWT-000000+0015-1230000-AIRWAV-";
        let mut bits = Vec::new();
        for ch in header.bytes() {
            for i in 0..8 {
                bits.push((ch >> i) & 1);
            }
        }
        let rate = 48_000u32;
        let n = (bits.len() as f64 / 520.83 * f64::from(rate)) as usize + 64;
        let mut bytes = vec![127u8; n * 2];
        for i in 0..n {
            let t = i as f64 / f64::from(rate);
            let bi = (t * 520.83).floor() as usize;
            if bi >= bits.len() {
                break;
            }
            let freq = if bits[bi] == 1 { 1562.5 } else { 2083.3 };
            let env = 0.5 + 0.5 * (TAU * freq * t).cos();
            write_iq(&mut bytes, i, 0.7 * env, 0.);
        }
        let mut dec = SameDecoder::default();
        let frames = dec.push(&bytes, 0, &rx(162_400_000, rate));
        assert!(
            frames.iter().any(|f| f.verified && f.protocol == "SAME"),
            "{frames:?}"
        );
    }
}
