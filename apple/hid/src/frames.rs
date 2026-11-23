//! Splitting the depacketizer's Annex B output into whole access units, and
//! the RTP sequence bookkeeping RTCP reports need.

/// Annex B start code and the access unit delimiter idevice's
/// `HevcDepacketizer` writes between pictures, in sequence order
const AUD: [u8; 7] = [0, 0, 0, 1, 0x46, 0x01, 0x50];

/// Collects depacketized bytes and hands out one picture at a time.
///
/// The depacketizer reorders packets and writes an AUD where a picture ends,
/// so splitting there never merges two pictures. A picture also ends at its
/// RTP marker, which lets it out a frame earlier, but only while packets
/// arrive in order: after a reordered or missing packet, the marker packet
/// may still wait in the depacketizer, and only the next AUD is safe.
#[derive(Default)]
pub struct AccessUnits {
    pending: Vec<u8>,
    last_seq: Option<u16>,
    in_order: bool,
}

impl AccessUnits {
    pub fn new() -> Self {
        Self {
            in_order: true,
            ..Self::default()
        }
    }

    /// Takes the output the depacketizer gave after the packet `seq`
    pub fn push(&mut self, seq: u16, marker: bool, output: &[u8]) -> Vec<Vec<u8>> {
        if self
            .last_seq
            .is_some_and(|last| seq != last.wrapping_add(1))
        {
            self.in_order = false;
        }
        if self.last_seq.is_none_or(|last| seq_after(seq, last)) {
            self.last_seq = Some(seq);
        }
        self.pending.extend_from_slice(output);

        let mut units = Vec::new();
        while let Some(at) = find(&self.pending, &AUD) {
            let rest = self.pending.split_off(at + AUD.len());
            self.pending.truncate(at);
            let unit = std::mem::replace(&mut self.pending, rest);
            if !unit.is_empty() {
                units.push(unit);
            }
            // The depacketizer has caught up to a picture boundary
            self.in_order = true;
        }
        if marker && self.in_order && !self.pending.is_empty() {
            units.push(std::mem::take(&mut self.pending));
        }
        units
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// RFC 3550's serial number order for 16-bit sequence numbers
fn seq_after(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

/// The extended highest sequence number for RTCP receiver reports: the
/// highest 16-bit number seen, plus 65,536 for each time it wrapped
#[derive(Default)]
pub struct ExtendedSeq {
    highest: Option<u16>,
    cycles: u32,
}

impl ExtendedSeq {
    pub fn update(&mut self, seq: u16) -> u32 {
        match self.highest {
            None => self.highest = Some(seq),
            Some(high) if seq_after(seq, high) => {
                if seq < high {
                    self.cycles = self.cycles.wrapping_add(1 << 16);
                }
                self.highest = Some(seq);
            }
            // Older or repeated packets leave the highest as it is
            Some(_) => {}
        }
        self.value()
    }

    pub fn value(&self) -> u32 {
        self.cycles | u32::from(self.highest.unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nal(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut v = vec![0, 0, 0, 1, kind << 1, 1];
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn splits_on_the_depacketizers_delimiters() {
        let mut units = AccessUnits::new();
        // One output holding the end of picture A, a delimiter and the start of B
        let a = nal(1, b"a");
        let b = nal(1, b"b");
        let mut out = a.clone();
        out.extend_from_slice(&AUD);
        out.extend_from_slice(&b);
        assert_eq!(units.push(10, false, &out), vec![a]);
        // B finishes at its marker, in order
        assert_eq!(units.push(11, true, &[]), vec![b]);
    }

    #[test]
    fn a_marker_after_reordering_waits_for_the_delimiter() {
        let mut units = AccessUnits::new();
        let a = nal(1, b"a");
        assert!(units.push(10, false, &a[..3]).is_empty());
        // Packet 12 (the marker) arrives before 11: the depacketizer holds it,
        // so its output is not the whole picture yet
        assert!(units.push(12, true, &[]).is_empty());
        let mut out = a[3..].to_vec();
        out.extend_from_slice(&AUD);
        assert_eq!(units.push(11, false, &out), vec![a]);
    }

    #[test]
    fn the_extended_sequence_counts_wraps_and_ignores_late_packets() {
        let mut seq = ExtendedSeq::default();
        assert_eq!(seq.update(65_534), 65_534);
        assert_eq!(seq.update(65_535), 65_535);
        assert_eq!(seq.update(0), 65_536);
        assert_eq!(seq.update(65_533), 65_536); // late, from before the wrap
        assert_eq!(seq.update(2), 65_538);
    }
}
