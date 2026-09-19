# Passive scope

AIRWAV supports one RTL-SDR Blog V4 receiver. No transmitter abstraction, generic SoapySDR source, network radio, exploitation, injection, jamming, spoofing, key recovery, authentication bypass, or protected-content circumvention is present.

The current milestone measures RF activity, stores IQ, and can derive AM/FM audio from a manually selected live or recorded channel. Live audio remains subject to physical receiver acceptance. It does not decode protocols, assign identities, derive geography, or track aircraft. Future protocol modules must be independently enableable and restricted to lawfully receivable open/unencrypted information. Protected content should yield observable protocol candidates and measurements only; payload interpretation stays unavailable.

There is no network requirement or enrichment service in the runtime. Source, receiver configuration, timestamps, measurement semantics and raw artifact hashes accompany captures. Fixture previews are labeled persistently in their manifests and terminal views.

Bias tee defaults off. Its value is explicit configuration, not a convenience action. The driver boundary reads the device's forced-bias EEPROM policy and refuses an off request that cannot be honored. AIRWAV never writes EEPROM. USB identity checks are compatibility checks, not cryptographic anti-counterfeit certification.
