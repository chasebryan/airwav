# Decoder contract — future milestone

There is no supported protocol decoder in this release. Signal frequency and SNR alone cannot verify a protocol. Confidence is shown as not established.

A decoder module must describe its accepted representation (raw IQ, channelized IQ, symbols, audio or frames), required sample rate/bandwidth, compatibility evidence, bounded resource use, enablement policy, state reset rules, and versioned output. Output must include protocol candidates, timestamp/sample position, independent verification status, raw frame reference, parsed fields, warnings, and a documented confidence calculation.

A first Mode S/1090ES implementation should narrowly specify which downlink formats and fields are verified. CRC success is evidence, not an invented classifier probability. An identity must point to a preserved frame and IQ/configuration epoch. Synthetic modulation vectors are useful unit tests but do not replace independent known-good recorded RF fixtures. Do not claim UAT, ACARS, VDL2 or AM monitoring because a descriptor exists.

Future interpretation tables should append new interpretations with supersession links; they must never rewrite original observations.
