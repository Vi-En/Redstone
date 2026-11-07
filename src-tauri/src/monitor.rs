use crate::server::FakeClock;

#[derive(Debug, Clone, PartialEq)]
pub struct RamSettings {
    system_ram_gb: f64,
    max_heap_mb: u64,
}

impl RamSettings {
    pub fn new(system_ram_gb: f64) -> Self {
        Self {
            system_ram_gb,
            max_heap_mb: 0,
        }
    }

    pub fn set_max_heap_mb(&mut self, max_heap_mb: u64) {
        self.max_heap_mb = max_heap_mb;
    }

    pub fn next_start_args(&self) -> Vec<String> {
        let gigabytes = (self.max_heap_mb / 1024).max(1);
        vec![format!("-Xmx{gigabytes}G")]
    }

    pub fn warning_for_user(&self) -> bool {
        if self.max_heap_mb == 0 {
            return false;
        }

        let system_ram_mb = self.system_ram_gb * 1024.0;
        (self.max_heap_mb as f64) > system_ram_mb * 0.8
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResourceMonitor {
    tps_threshold: f64,
    alert_window_ms: u64,
    samples: Vec<(u64, f64)>,
    alert_active: bool,
}

impl ResourceMonitor {
    pub fn new(_clock: FakeClock, tps_threshold: f64, alert_window_seconds: u64) -> Self {
        Self {
            tps_threshold,
            alert_window_ms: alert_window_seconds.saturating_mul(1000),
            samples: Vec::new(),
            alert_active: false,
        }
    }

    pub fn parse_tps(&self, line: &str) -> Option<f64> {
        let lowercase = line.to_ascii_lowercase();

        for marker in ["tps:", "/tick query output:"] {
            if let Some(index) = lowercase.find(marker) {
                let rest = &line[index + marker.len()..];
                let value = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .trim_matches(|ch: char| !ch.is_ascii_digit() && ch != '.');
                return value.parse::<f64>().ok();
            }
        }

        None
    }

    pub fn record_sample(&mut self, tps: f64, timestamp_ms: u64) -> bool {
        self.samples.retain(|(sample_time, _)| {
            timestamp_ms.saturating_sub(*sample_time) <= self.alert_window_ms
        });
        self.samples.push((timestamp_ms, tps));

        let window = self
            .samples
            .iter()
            .filter(|(sample_time, _)| {
                timestamp_ms.saturating_sub(*sample_time) <= self.alert_window_ms
            })
            .collect::<Vec<_>>();

        self.alert_active = !window.is_empty()
            && window
                .iter()
                .all(|(_, sample_tps)| *sample_tps < self.tps_threshold)
            && timestamp_ms.saturating_sub(
                window
                    .first()
                    .map(|(sample_time, _)| *sample_time)
                    .unwrap_or(timestamp_ms),
            ) >= self.alert_window_ms;

        self.alert_active
    }

    pub fn alert_active(&self) -> bool {
        self.alert_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tps_parser_handles_tick_query_output_samples() {
        let monitor = ResourceMonitor::new(FakeClock::default(), 15.0, 10);

        assert_eq!(
            monitor.parse_tps("[12:00:00] [INFO]: TPS: 19.92"),
            Some(19.92)
        );
        assert_eq!(
            monitor.parse_tps("[12:00:00] /tick query output: 14.30"),
            Some(14.3)
        );
    }

    #[test]
    fn alert_fires_after_tps_stays_below_threshold_for_long_enough() {
        let mut clock = FakeClock::default();
        let mut monitor = ResourceMonitor::new(clock, 15.0, 10);

        monitor.record_sample(14.9, clock.now_ms());
        clock.advance(5000);
        monitor.record_sample(14.8, clock.now_ms());
        clock.advance(5000);
        monitor.record_sample(14.7, clock.now_ms());

        assert!(monitor.alert_active());
    }

    #[test]
    fn changing_ram_slider_sets_jvm_xmx_and_warns_when_too_high() {
        let system_ram_gb = 16.0;
        let mut settings = RamSettings::new(system_ram_gb);

        settings.set_max_heap_mb(12288);
        let args = settings.next_start_args();

        assert!(args.iter().any(|arg| arg == "-Xmx12G"));
        assert!(!settings.warning_for_user());

        settings.set_max_heap_mb(14000);
        assert!(settings.warning_for_user());
        assert!(settings
            .next_start_args()
            .iter()
            .any(|arg| arg == "-Xmx13G"));
    }
}
