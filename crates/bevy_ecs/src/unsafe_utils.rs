/// A set of checked unsafe extension methods.
pub(crate) trait UnsafeVecExt<T> {
    /// Removes an element from the vector and returns it.
    ///
    /// The removed element is replaced by the last element in the vector.
    ///
    /// This does not preserve ordering, but is O(1).
    ///
    /// In release builds, this will not panic if `index` is invalid and
    /// will not do any bounds checking.
    ///
    /// # Panics
    /// Will panic in debug builds if `index` is invalid.
    ///
    /// # Safety
    /// index must be less than the length of the [`Vec`].
    unsafe fn swap_remove_unchecked(&mut self, index: usize) -> T;
}

impl<T> UnsafeVecExt<T> for Vec<T> {
    #[inline]
    unsafe fn swap_remove_unchecked(&mut self, index: usize) -> T {
        let len = self.len();
        debug_assert!(index < len);
        let value = core::ptr::read(self.as_ptr().add(index));
        let base_ptr = self.as_mut_ptr();
        core::ptr::copy(base_ptr.add(len - 1), base_ptr.add(index), 1);
        self.set_len(len - 1);
        value
    }
}
