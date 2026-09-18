# AIRWAV web observer

**DEMO FIXTURE only.** This is not a live receiver, does not load librtlsdr, does not talk to USB, and is not a production interface. It runs the same class of synthetic IQ the native `make_fixture` example generates so the measurement UI can be inspected without a V4.

Detected activity is `UNKNOWN`. The observer does not invent aircraft, identities, protocol confidence, decoder output, PRISM channels, or MAX-I scores.

```bash
npm install
npm run dev
```

`npm run build` writes a static bundle to `dist/`.
