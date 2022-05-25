/// Gets the bit at position `n`.
/// Bits are numbered from 0 (least significant) to 7 (most significant).
pub fn get_bit_at(input: u8, n: u8) -> bool {
  if n < 8 {
    input & (1 << n) != 0
  } else {
    false
  }
}

