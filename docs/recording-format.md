# AWR v1 — AIRWAV Recording

An `.awr` is a directory bundle, opened incrementally. It is not a compressed file.

```text
session.awr/
  manifest.json        format/version, session ID, source, receiver, completion
  session.json         effective capture configuration
  observations.jsonl   measured spectra, islands, metrics, receiver, host timestamps
  events.jsonl         capture ID, trigger position, IQ hash/index, measured evidence
  index.sqlite         schema v1 metadata index (WAL during recording)
  iq/
    AW-EVENT-000001.cu8
```

`manifest.format` is `airwav-recording`, `version` is `1`, and `sample_format` is `cu8-interleaved`. Unknown versions/formats fail closed. `source.kind` is `LiveV4` or `DemoFixture`; the latter must always display DEMO FIXTURE. Runtime receiver capture has no fixture fallback. No synthetic aircraft or interpretations are populated.

IQ is unsigned 8-bit, interleaved I then Q, without a binary header. Event chunk entries contain the first sample index, host receipt time, file byte offset and byte count. Every event carries a BLAKE3 digest and length. Sample indices include known application drops; they are not hardware timestamps. Host timestamps are nanoseconds since Unix epoch and may be affected by clock adjustments. Wall-clock replay clamps backwards timestamp differences to zero.

Capture includes the currently available ring, then the requested post-roll. An early capture may have less pre-roll than configured. Post-roll is truncated exactly at its target sample. A discontinuity or explicit stop marks the event incomplete. At most one event collects IQ at a time. AWR has no claim of forensic certification.

Journal rows are newline-delimited JSON and at most 2 MiB. Readers bound allocation, validate spectrum sizes/numeric fields and stream IQ hashing in 64 KiB chunks. Artifact references must remain inside the recording directory. `inspect` verifies each complete event artifact. Event verification does not authenticate the author or prove the capture was received over RF.

Each metadata row is synchronized before its SQLite index entry. During capture, the IQ artifact ends in `.partial`. The writer synchronizes and renames it before publishing the event row. The complete manifest is installed by atomic rename only after normal finalization. This preserves previously finalized events after a crash; the current unfinalized IQ artifact may remain without a reconstructible chunk index.

`recover INPUT --output NEW.awr` requires an incomplete source, copies complete journal rows and verifies/copies committed events into a new bundle. A torn final JSONL line is skipped; malformed interior rows fail. Uncommitted `.partial` artifacts remain in the original for manual inspection; they are not silently promoted to verified events. Recovery never overwrites the source or a destination that already exists.

Disk limits include all bundle files and reserve space for finalization. Free space is checked before writes. No rotation, deletion, automatic cleanup, or indefinite continuous IQ capture is performed in this milestone. Those policies require explicit later implementation.

Replay renders semantic observations at recorded intervals with 0.25× through 4×, pause and single-step. It does not re-run a decoder. Event navigation scans the journal without loading it all into memory; the interactive event index is limited to 4,096 events. `inspect` streams all events. The current UI waterfall retains 160 rows. Export uses the same renderer and writes SVG plus JSON metadata; PNG/video/cast are later milestones.
