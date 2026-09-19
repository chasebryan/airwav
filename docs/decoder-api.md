# Decoder contract

AIRWAV emits a protocol tag only after an independent check succeeds. Signal frequency and SNR never verify a protocol. Confidence is `verified` or it is not established (`UNKNOWN`).

A decoder module must describe its accepted representation (raw IQ, channelized IQ, symbols, audio or frames), required sample rate/bandwidth, compatibility evidence, bounded resource use, enablement policy, state reset rules, and versioned output. Output includes protocol candidates, timestamp/sample position, independent verification status, raw frame reference, parsed fields, warnings, and a documented check (CRC remainder, BCH, FCS, or framing).

| Module | Window | Check | Claimed fields |
| --- | --- | --- | --- |
| Mode S / 1090ES | ± sample-rate of 1090 MHz | CRC-24 remainder 0, poly 0xFFF409 | DF, ICAO (DF 11/17/18), callsign on DF17 TC 1–4 |
| ACARS | 118–138 MHz | odd parity per character + XOR block checksum | aircraft, label, text |
| APRS | 144–148 MHz | AX.25 HDLC CRC-16-CCITT after destuff | from, to, info |
| POCSAG | 433.92 ± 300 kHz, or 150–174 MHz off NOAA | BCH(31,21)+parity remainder 0 | text, valid codeword count |
| SAME | 162.3–162.6 MHz | recovered `ZCZC-` header from 520.83 baud AFSK | header |

CRC success is evidence, not an invented classifier probability. An identity must point to a preserved frame and IQ/configuration epoch. Synthetic modulation vectors are unit tests; they do not replace independent known-good recorded RF fixtures.

A retune, sample-rate change, or decoder disable **resets decoder state**. Future interpretation tables should append new interpretations with supersession links; they must never rewrite original observations.

Do not claim UAT, VDL2, or AM voice identification because a descriptor or bookmark exists. FM broadcast occupancy has no RDS decoder in this milestone.
