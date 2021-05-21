use super::{
    parse_16_chars, parse_2_chars, parse_32_chars, parse_4_chars, parse_8_chars, trees::*,
    ParseIntErrorPublic, MINUS, PLUS,
};
use core::num::IntErrorKind;

#[cfg(feature = "core_intrinsics")]
macro_rules! likely {
    ($e:expr) => {
        core::intrinsics::likely($e)
    };
}
#[cfg(not(feature = "core_intrinsics"))]
macro_rules! likely {
    ($e:expr) => {
        $e
    };
}

#[cfg(feature = "core_intrinsics")]
macro_rules! unlikely {
    ($e:expr) => {
        core::intrinsics::unlikely($e)
    };
}
#[cfg(not(feature = "core_intrinsics"))]
macro_rules! unlikely {
    ($e:expr) => {
        $e
    };
}

type Pie = ParseIntErrorPublic;

#[doc(hidden)]
pub trait FromStrRadixHelper: PartialOrd + Copy + 'static {
    const MINIMUM: Self;
    const BITS_COUNT: u32;
    const FIRST_SIG: u8;
    const TAIL: Self;
    const TREE: &'static [Self];
    const CHARS: usize;
    fn from_u128(u: u128) -> Self;
    fn from_u64(u: u64) -> Self;
    fn from_u32(u: u32) -> Self;
    fn from_u16(u: u16) -> Self;
    fn from_u8(u: u8) -> Self;

    fn mul_checked(&self, other: Self) -> Option<Self>;
    fn sub_checked(&self, other: Self) -> Option<Self>;
    fn add_checked(&self, other: Self) -> Option<Self>;
    /// # Safety
    /// Caller needs to ensure no overflow can occur.
    unsafe fn mul_unchecked(&self, other: Self) -> Self;
    /// # Safety
    /// Caller needs to ensure no overflow can occur.
    unsafe fn sub_unchecked(&self, other: Self) -> Self;
    /// # Safety
    /// Caller needs to ensure no overflow can occur.
    unsafe fn add_unchecked(&self, other: Self) -> Self;
}

macro_rules! doit {
    ($($t:ty,$tr:expr,$chars:literal,$first_sig:literal,$tail:literal)*) => ($(impl FromStrRadixHelper for $t {
        const MINIMUM: Self = Self::MIN;
        const FIRST_SIG: u8 = $first_sig;
        const TAIL: Self = $tail;
        const BITS_COUNT: u32 = Self::BITS;
        const TREE: &'static[Self] = $tr;
        const CHARS: usize = $chars;
        #[inline(always)]
        fn from_u128(u: u128) -> Self { u as Self }
        #[inline(always)]
        fn from_u64(u: u64) -> Self { u as Self }
        #[inline(always)]
        fn from_u32(u: u32) -> Self { u as Self }
        #[inline(always)]
        fn from_u16(u: u16) -> Self { u as Self }
        #[inline(always)]
        fn from_u8(u: u8) -> Self { u as Self }
        #[inline(always)]
        fn mul_checked(&self, other: Self) -> Option<Self> {
            Self::checked_mul(*self, other as Self)
        }
        #[inline(always)]
        fn sub_checked(&self, other: Self) -> Option<Self> {
            Self::checked_sub(*self, other as Self)
        }
        #[inline(always)]
        fn add_checked(&self, other: Self) -> Option<Self> {
            Self::checked_add(*self, other as Self)
        }
        #[inline(always)]
        unsafe fn mul_unchecked(&self, other: Self) -> Self {
            Self::wrapping_mul(*self, other as Self)
        }
        #[inline(always)]
        unsafe fn sub_unchecked(&self, other: Self) -> Self {
            Self::wrapping_sub(*self, other as Self)
        }
        #[inline(always)]
        unsafe fn add_unchecked(&self, other: Self) -> Self {
            Self::wrapping_add(*self, other as Self)
        }
    })*)
}

doit! {
    i8,TENS_I8,3,1,-28
    i16,TENS_I16,5,3,-2768
    i32,TENS_I32,10,2,-147_483_648
    i64,TENS_I64,19,9,-223_372_036_854_775_808
    i128,TENS_I128,39,1,-70141183460469231731687303715884105728
    u8,TENS_U8,3,2,55
    u16,TENS_U16,5,6,5535
    u32,TENS_U32,10,4,294_967_295
    u64,TENS_U64,20,1,8_446_744_073_709_551_615
    u128,TENS_U128,39,3,40_282_366_920_938_463_463_374_607_431_768_211_455
}

#[cfg(target_pointer_width = "16")] //E.g. msp430-none-elf micro-controller.
doit! {
    isize,TENS_ISIZE,5,3,-2768
    usize,TENS_USIZE,5,6,5535
}

#[cfg(target_pointer_width = "32")]
doit! {
    isize,TENS_ISIZE,10,2,-147_483_648
    usize,TENS_USIZE,10,4,294_967_295
}

#[cfg(target_pointer_width = "64")]
doit! {
    isize,TENS_ISIZE,19,9,-223_372_036_854_775_808
    usize,TENS_USIZE,20,1,8_446_744_073_709_551_615
}

// u128: 0 to 340_282_366_920_938_463_463_374_607_431_768_211_455
// (39 digits!)
#[doc(hidden)]
pub fn parse_challenger<T>(s: &[u8]) -> Result<T, Pie>
where
    T: FromStrRadixHelper,
{
    parse(s)
}

// u128: 0 to 340_282_366_920_938_463_463_374_607_431_768_211_455
// (39 digits!)

macro_rules! invalid {
    () => {
        Pie {
            kind: IntErrorKind::InvalidDigit,
        }
    };
}
macro_rules! empty {
    () => {
        Pie {
            kind: IntErrorKind::Empty,
        }
    };
}

macro_rules! pos_overflow {
    () => {
        Pie {
            kind: IntErrorKind::PosOverflow,
        }
    };
}

macro_rules! neg_overflow {
    () => {
        Pie {
            kind: IntErrorKind::NegOverflow,
        }
    };
}

/// Parses a UTF8 String as a number.
///
/// Takes a `&[u8]` so that it can be used in no_std contexts also
/// (Call `.as_bytes()` on the String).
///
/// It has exactly the same semantics as `std::str::parse`,
/// but faster. (compiled with nightly,simd features
/// and target native cpu will get the absolute fastest result.)
///
/// Positives are slightly faster than negatives when parsing and
/// if you don't need to put a leading `+` then that will be faster too.
///
/// # Examples
///
/// ```rust
/// let s: String = "+000123".into();
/// assert_eq!(atoi_radix10::parse::<u8>(s.as_bytes()), Ok(123));
/// ```
pub fn parse<T>(mut s: &[u8]) -> Result<T, Pie>
where
    T: FromStrRadixHelper,
{
    let is_signed_ty = T::from_u32(0) > T::MINIMUM;
    let mut checked: Option<u8> = None;
    if let Some(val) = s.get(0) {
        let mut val = val.wrapping_sub(b'0');
        loop {
            if likely!(val <= 9) {
                // positive without +. could be long with lots of leading zeros.
                loop {
                    let l = s.len();
                    if likely!(l < T::CHARS) {
                        let mut res = T::from_u8(0);
                        let l_1 = l & 1 != 0 && T::BITS_COUNT >= 4;
                        let l_2 = l & 2 != 0 && T::BITS_COUNT >= 8;
                        let l_4 = l & 4 != 0 && T::BITS_COUNT >= 16;
                        let l_8 = l & 8 != 0 && T::BITS_COUNT >= 32;
                        let l16 = l & 16 != 0 && T::BITS_COUNT >= 64;
                        let l32 = l & 32 != 0 && T::BITS_COUNT >= 128;

                        unsafe {
                            if l_1 {
                                let val_t = T::from_u8(val);
                                s = &s.get_unchecked(1..);
                                if s.is_empty() {
                                    return Ok(val_t);
                                }
                                res = val_t.mul_unchecked(*T::TREE.get_unchecked(s.len()));
                            }
                            if l_2 {
                                let val = T::from_u16(parse_2_chars(&s)?);
                                s = &s.get_unchecked(2..);
                                if s.is_empty() {
                                    res = res.add_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.add_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if l_4 {
                                let val = T::from_u16(parse_4_chars(&s)?);
                                s = &s.get_unchecked(4..);
                                if s.is_empty() {
                                    res = res.add_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.add_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if l_8 {
                                let val = T::from_u32(parse_8_chars(&s)?);
                                s = &s.get_unchecked(8..);
                                if s.is_empty() {
                                    res = res.add_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.add_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if l16 {
                                let val = T::from_u64(parse_16_chars(&s)?);
                                s = &s.get_unchecked(16..);
                                if s.is_empty() {
                                    res = res.add_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.add_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if l32 {
                                let val = T::from_u128(parse_32_chars(&s)?);
                                res = res.add_unchecked(val);
                            }
                            return if let Some(checked) = checked {
                                // SAFETY: mul is in range as `checked` is constrained to <= T::FIRST_SIG
                                let checked = T::from_u8(checked)
                                    .mul_unchecked(*T::TREE.get_unchecked(T::CHARS - 1));
                                checked.add_checked(res).ok_or(pos_overflow!())
                            } else {
                                Ok(res)
                            };
                        }
                    }
                    // Deal with edge cases then get back to the top,
                    if l == T::CHARS && val <= T::FIRST_SIG {
                        checked = Some(val);
                        s = &s[1..];
                        val = unsafe { s.get_unchecked(0).wrapping_sub(b'0') };
                        if val > 9 {
                            return Err(invalid!());
                        }
                    } else if val == 0 {
                        // Remove leading zeros
                        val = b'0';
                        while val == b'0' {
                            s = &s[1..];
                            val = match s.get(0) {
                                Some(val) => *val,
                                None => return Ok(T::from_u8(0)),
                            }
                        }
                        val = val.wrapping_sub(b'0');
                        if val > 9 {
                            return Err(empty!());
                        }
                    } else {
                        return Err(pos_overflow!());
                    }

                    debug_assert!(val <= 9);
                }
            } else if likely!(is_signed_ty && val == MINUS) {
                s = &s[1..];

                // negative without -. could be long with lots of leading zeros.
                loop {
                    let l = s.len();
                    if likely!(l < T::CHARS && l != 0) {
                        let mut res = T::from_u8(0);
                        unsafe {
                            if (l & 1) != 0 && T::BITS_COUNT >= 4 {
                                let val = s.get_unchecked(0).wrapping_sub(b'0');
                                res = res.sub_unchecked(T::from_u8(val));
                                if likely!(val <= 9 && l == 1) {
                                    return Ok(res);
                                } else if likely!(val <= 9) {
                                    s = &s[1..];
                                    res = res.mul_unchecked(*T::TREE.get_unchecked(s.len()));
                                } else {
                                    return Err(invalid!());
                                };
                            }
                            if (l & 2 != 0) && T::BITS_COUNT >= 8 {
                                let val = T::from_u16(parse_2_chars(&s)?);
                                s = &s[2..];
                                if s.is_empty() {
                                    res = res.sub_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.sub_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if (l & 4) != 0 && T::BITS_COUNT >= 16 {
                                let val = T::from_u16(parse_4_chars(&s)?);
                                s = &s[4..];
                                if s.is_empty() {
                                    res = res.sub_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.sub_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if (l & 8) != 0 && T::BITS_COUNT >= 32 {
                                let val = T::from_u32(parse_8_chars(&s)?);
                                s = &s[8..];
                                if s.is_empty() {
                                    res = res.sub_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.sub_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if (l & 16) != 0 && T::BITS_COUNT >= 64 {
                                let val = T::from_u64(parse_16_chars(&s)?);
                                s = &s[16..];
                                if s.is_empty() {
                                    res = res.sub_unchecked(val);
                                    if checked.is_none() {
                                        return Ok(res);
                                    }
                                } else {
                                    res = res.sub_unchecked(
                                        T::TREE.get_unchecked(s.len()).mul_unchecked(val),
                                    );
                                }
                            }
                            if (l & 32) != 0 && T::BITS_COUNT >= 128 {
                                res = res.sub_unchecked(T::from_u128(parse_32_chars(&s)?));
                            }

                            return if let Some(chk) = checked {
                                if unlikely!(res == T::TAIL && chk == T::FIRST_SIG) {
                                    return Ok(T::MINIMUM);
                                }
                                // SAFETY: mul is in range as `checked` is constrained to <= T::FIRST_SIG
                                let val = T::from_u8(chk)
                                    .mul_unchecked(*T::TREE.get_unchecked(T::CHARS - 1));
                                res.sub_checked(val).ok_or(neg_overflow!())
                            } else {
                                Ok(res)
                            };
                        }
                    }
                    val = if let Some(val) = s.get(0) {
                        *val
                    } else {
                        return Err(empty!());
                    };
                    val = val.wrapping_sub(b'0');
                    if l == T::CHARS && val <= T::FIRST_SIG {
                        checked = Some(val);
                        s = &s[1..];
                    } else if val == 0 {
                        val = b'0';
                        while val == b'0' {
                            s = &s[1..];
                            val = match s.get(0) {
                                Some(val) => *val,
                                None => return Ok(T::from_u8(0)),
                            }
                        }
                    } else {
                        return Err(neg_overflow!());
                    }
                }
            } else if val == PLUS {
                s = &s[1..];
                val = match s.get(0) {
                    Some(value) => {
                        let value = value.wrapping_sub(b'0');
                        if likely!(value <= 9) {
                            value
                        } else {
                            return Err(empty!());
                        }
                    }
                    None => return Err(empty!()),
                };
            } else {
                return Err(invalid!());
            }
        }
    } else {
        Err(empty!())
    }
}
