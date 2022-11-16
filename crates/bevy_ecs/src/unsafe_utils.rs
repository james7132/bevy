/// A debug checked version of [`Option::unwrap_unchecked`]. Will panic in
/// debug modes if unwrapping a `None` or `Err` value in debug mode, but is
/// equivalent to `Option::unwrap_uncheched` or `Result::unwrap_unchecked`
/// in release mode.
pub(crate) trait DebugCheckedUnwrap {
    type Item;
    /// # Panics
    /// Panics if the value is `None` or `Err`, only in debug mode.
    ///
    /// # Safety
    /// This must never be called on a `None` or `Err` value. This can
    /// only be called on `Some` or `Ok` values.
    unsafe fn debug_checked_unwrap(self) -> Self::Item;
}

// Thes two impls are explicitly split to ensure that the unreachable! macro
// does not cause inlining to fail when compiling in release mode.
#[cfg(debug_assertions)]
impl<T> DebugCheckedUnwrap for Option<T> {
    type Item = T;

    #[inline(always)]
    #[track_caller]
    unsafe fn debug_checked_unwrap(self) -> Self::Item {
        if let Some(inner) = self {
            inner
        } else {
            unreachable!()
        }
    }
}

#[cfg(not(debug_assertions))]
impl<T> DebugCheckedUnwrap for Option<T> {
    type Item = T;

    #[inline(always)]
    unsafe fn debug_checked_unwrap(self) -> Self::Item {
        if let Some(inner) = self {
            inner
        } else {
            std::hint::unreachable_unchecked()
        }
    }
}
