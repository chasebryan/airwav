    #[test]
    fn desired_audio_survives_a_simultaneous_adjacent_channel() {
        let rate = 1_024_000;
        for (mode, neighbor_hz) in [(AudioMode::Am, 50_000.), (AudioMode::Fm, 300_000.)] {
            let make_block = |with_neighbor: bool| IqBlock {
                first_sample: 0,
                received_ns: 0,
                bytes: (0..rate / 10)
                    .flat_map(|i| {
                        let t = i as f64 / rate as f64;
                        let desired_tone = TAU * 1_000. * t;
                        let adjacent_tone = TAU * 2_000. * t;
                        let desired = match mode {
                            AudioMode::Am => {
                                Complex64::from_polar(0.3 * (1. + 0.5 * desired_tone.cos()), 0.)
                            }
                            AudioMode::Fm => {
                                Complex64::from_polar(0.3, 50_000. / 1_000. * desired_tone.sin())
                            }
                            AudioMode::Nfm => unreachable!(),
                        };
                        let adjacent = if with_neighbor {
                            match mode {
                                AudioMode::Am => Complex64::from_polar(
                                    0.3 * (1. + 0.5 * adjacent_tone.cos()),
                                    TAU * neighbor_hz * t,
                                ),
                                AudioMode::Fm => Complex64::from_polar(
                                    0.3,
                                    TAU * neighbor_hz * t + 50_000. / 2_000. * adjacent_tone.sin(),
                                ),
                                AudioMode::Nfm => unreachable!(),
                            }
                        } else {
                            Complex64::new(0., 0.)
                        };
                        let iq = desired + adjacent;
                        [
                            (127.5 + 128. * iq.re).round().clamp(0., 255.) as u8,
                            (127.5 + 128. * iq.im).round().clamp(0., 255.) as u8,
                        ]
                    })
                    .collect(),
            };
            let decode = |block: IqBlock| {
                let mut pcm = vec![];
                AudioDemodulator::new(config(mode, rate, 0.))
                    .unwrap()
                    .push(&block, &mut pcm)
                    .unwrap();
                pcm
            };
            let baseline = decode(make_block(false));
            let mixed = decode(make_block(true));
            let desired = amplitude(&mixed[960..], 1_000.);
            let leakage = amplitude(&mixed[960..], 2_000.);
            let baseline_desired = amplitude(&baseline[960..], 1_000.);
            let baseline_leakage = amplitude(&baseline[960..], 2_000.);
            assert!(
                desired > baseline_desired * 0.75,
                "{mode:?}: desired {desired} versus {baseline_desired}"
            );
            assert!(
                leakage < desired * 0.1 && leakage < baseline_leakage + 0.03,
                "{mode:?}: adjacent leakage {leakage} versus desired {desired}, baseline {baseline_leakage}"
            );
        }
    }
