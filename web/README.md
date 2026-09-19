# AIRWAV observer

Browser observation workstation for **synthetic IQ** and dropped unsigned IQ files. Same tuner, FFT, Signal Islands, and CRC-gated decoders as native AIRWAV.

It is **not** a live USB RTL-SDR Blog V4, does not load librtlsdr, and does not transmit. Protocol tags require CRC or parity. Frequency coincidence is not identity.

```bash
npm install
npm run dev
```

Tune with `/` or the VFO, `n`/`N` to step, Shift-click the spectrum to retune, or load a `.cu8` / `.u8` / `.iq` file. Bookmarks: VHF air, ACARS 131.550, APRS 144.390, NOAA 162.400, FM, 433 ISM, 1090 ES.

`npm run build` writes a static bundle to `dist/`.
