    pub fn handle(&mut self, event: Event) -> Action {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char('a') => Action::AudioToggle,
                KeyCode::Char('m') => Action::AudioMode,
                KeyCode::Char('9') => Action::AudioVolume(-10),
                KeyCode::Char('0') => Action::AudioVolume(10),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
                KeyCode::Esc => {
                    self.view = None;
                    Action::None
                }
                KeyCode::Char('?') => {
                    self.view = Some(View::Help);
                    Action::None
                }
                KeyCode::Char('d') => {
                    self.view = Some(View::Diagnostics);
                    Action::None
                }
                KeyCode::Char('e') => {
                    self.view = Some(View::Events);
                    Action::None
                }
                KeyCode::Char('i') | KeyCode::Enter => {
                    self.view = Some(View::Evidence);
                    Action::None
                }
                KeyCode::Tab => {
                    self.focused = (self.focused + 1) % 3;
                    Action::None
                }
                KeyCode::BackTab => {
                    self.focused = (self.focused + 2) % 3;
                    Action::None
                }
                KeyCode::Down => {
                    self.select(1);
                    Action::None
                }
                KeyCode::Up => {
                    self.select(-1);
                    Action::None
                }
                KeyCode::Char('r') if !self.replay => Action::Record,
                KeyCode::Char('c') if !self.replay => Action::Capture,
                KeyCode::Char(' ') => Action::Pause,
                KeyCode::Char('.') if self.replay => Action::Step,
                KeyCode::Char('[') if self.replay => Action::PreviousEvent,
                KeyCode::Char(']') if self.replay => Action::NextEvent,
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.zoom = (self.zoom * 2.).min(16.);
                    Action::None
                }
                KeyCode::Char('-') => {
                    self.zoom = (self.zoom / 2.).max(1.);
                    Action::None
                }
                KeyCode::Left => {
                    self.pan = (self.pan - 0.1 / self.zoom).max(0.);
                    Action::None
                }
                KeyCode::Right => {
                    self.pan = (self.pan + 0.1 / self.zoom).min(1.);
                    Action::None
                }
                KeyCode::Char(n @ '1'..='5') if self.replay => {
                    Action::Speed([0.25, 0.5, 1., 2., 4.][n as usize - '1' as usize])
                }
                KeyCode::Char('t') => {
                    self.cycle_theme();
                    Action::None
                }
                KeyCode::F(10) => {
                    self.demo = !self.demo;
                    Action::None
                }
                KeyCode::F(12) => Action::Screenshot,
                _ => Action::None,
            },
            Event::Mouse(mouse) => {
                let point = (mouse.column, mouse.row);
                self.hover = Some(point);
                if self.view.is_some() {
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                        && contains(self.areas.close, point)
                    {
                        self.view = None;
                    }
                    return Action::None;
                }
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        for (r, action) in &self.areas.buttons {
                            if contains(*r, point) {
                                return match action.clone() {
                                    Action::Open(view) => {
                                        self.view = Some(view);
                                        Action::None
                                    }
                                    Action::Theme => {
                                        self.cycle_theme();
                                        Action::None
                                    }
                                    Action::Demo => {
                                        self.demo = !self.demo;
                                        Action::None
                                    }
                                    Action::CycleSpeed => {
                                        let speeds = [0.25, 0.5, 1., 2., 4.];
                                        let i = speeds
                                            .iter()
                                            .position(|s| *s == self.speed)
                                            .unwrap_or(2);
                                        Action::Speed(speeds[(i + 1) % 5])
                                    }
                                    other => other,
                                };
                            }
                        }
                        if contains(self.areas.signals, point)
                            && mouse.row > self.areas.signals.y + 2
                        {
                            self.selected = self.signal_scroll
                                + (mouse.row - self.areas.signals.y - 3) as usize;
                            self.select(0);
                            if self.last_click.is_some_and(|(x, y, t)| {
                                x == point.0
                                    && y == point.1
                                    && t.elapsed() < Duration::from_millis(400)
                            }) {
                                self.view = Some(View::Evidence);
                            }
                            self.last_click = Some((point.0, point.1, Instant::now()));
                        }
                    }
                    MouseEventKind::Down(MouseButton::Right) => {
                        if contains(self.areas.signals, point) {
                            self.selected = self.signal_scroll
                                + mouse.row.saturating_sub(self.areas.signals.y + 3) as usize;
                            self.select(0);
                            self.view = Some(View::Evidence);
                        }
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        let direction = if mouse.kind == MouseEventKind::ScrollUp {
                            1.
                        } else {
                            -1.
                        };
                        if contains(self.areas.spectrum, point) {
                            if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                                self.pan = (self.pan + direction * 0.1 / self.zoom).clamp(0., 1.);
                            } else {
                                self.zoom = (self.zoom * 2f64.powf(direction)).clamp(1., 16.);
                            }
                        } else {
                            self.select(-direction as i32);
                        }
                    }
                    _ => {}
                }
                Action::None
            }
            _ => Action::None,
        }
    }
