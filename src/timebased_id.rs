use std::time::{SystemTime, UNIX_EPOCH};

use system_uptime::get_os_uptime;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct TimeBasedId(u128);

impl Default for TimeBasedId {
    fn default() -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time early unix epoch");
        let now_nanoseconds = duration.as_nanos();

        let uptime = get_os_uptime().expect("Uptime request failed") as u128;
        let uptime = uptime << 64;

        Self(uptime + now_nanoseconds)
    }
}

impl TimeBasedId {
    pub fn value(&self) -> u128 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_constructor() {
        let id = TimeBasedId::default();

        assert!(id.value() > 0);
    }

    #[test]
    fn test_sequence_of_unique_id() {
        let id1 = TimeBasedId::default();
        let id2 = TimeBasedId::default();

        assert!(id1 != id2);
    }
}
