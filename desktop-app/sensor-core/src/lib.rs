//! Read-only hardware decoding shared by the integrated sensor service.

pub fn intel_package_celsius(target_msr: u64, status_msr: u64) -> Option<f32> {
    let tj_max = ((target_msr >> 16) & 0xff) as u8;
    let valid = (status_msr & (1 << 31)) != 0;
    let distance = ((status_msr >> 16) & 0x7f) as u8;
    if !valid || !(60..=125).contains(&tj_max) || distance > tj_max {
        return None;
    }
    let celsius = tj_max - distance;
    (celsius > 0).then_some(celsius as f32)
}

#[cfg(windows)]
pub mod pawnio;

#[cfg(test)]
mod tests {
    use super::intel_package_celsius;

    #[test]
    fn accepts_a_valid_intel_package_reading() {
        let target = 100u64 << 16;
        let status = (1u64 << 31) | (45u64 << 16);
        assert_eq!(intel_package_celsius(target, status), Some(55.0));
    }

    #[test]
    fn rejects_invalid_or_implausible_readings() {
        let target = 100u64 << 16;
        assert_eq!(intel_package_celsius(target, 45u64 << 16), None);
        assert_eq!(intel_package_celsius(0, (1u64 << 31) | (45u64 << 16)), None);
        assert_eq!(
            intel_package_celsius(target, (1u64 << 31) | (120u64 << 16)),
            None
        );
    }
}
