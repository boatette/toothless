use std::fs;

/// Busy and total jiffies across all cores, from the first line of /proc/stat.
fn parse(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().next()?.strip_prefix("cpu ")?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .map(|f| f.parse().ok())
        .collect::<Option<_>>()?;
    // user nice system idle iowait irq softirq steal; guest time is already counted in user
    let total: u64 = fields.iter().take(8).sum();
    let idle = fields.get(3)? + fields.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

#[derive(Default)]
pub struct Cpu {
    last: Option<(u64, u64)>,
}

impl Cpu {
    /// Usage in percent since the previous call; None on the first call or when
    /// /proc/stat can't be read.
    pub fn sample(&mut self) -> Option<f32> {
        let now = parse(&fs::read_to_string("/proc/stat").ok()?)?;
        let usage = self.usage(now);
        self.last = Some(now);
        usage
    }

    fn usage(&self, (busy, total): (u64, u64)) -> Option<f32> {
        let (last_busy, last_total) = self.last?;
        let elapsed = total.checked_sub(last_total).filter(|&t| t > 0)?;
        Some(busy.saturating_sub(last_busy) as f32 * 100.0 / elapsed as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_aggregate_line() {
        let stat = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4 5 6 7 8 9 10\n";
        assert_eq!(Some((150, 1000)), parse(stat));
    }

    #[test]
    fn usage_is_the_busy_share_since_last_time() {
        let cpu = Cpu {
            last: Some((150, 1000)),
        };
        assert_eq!(Some(25.0), cpu.usage((400, 2000)));
        assert_eq!(None, Cpu::default().usage((400, 2000)));
    }
}
