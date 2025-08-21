use crate::{query::DebugCheckedUnwrap, storage::thin_array_ptr::ThinArrayPtr};
use core::{fmt, num::NonZero, ops::{Deref, DerefMut}};

pub struct Vec32<T> {
    data: ThinArrayPtr<T>,
    length: u32,
    capacity: u32,
}

unsafe impl<T: Send> Send for Vec32<T> {}
unsafe impl<T: Sync> Sync for Vec32<T> {}

impl<T> Vec32<T> {
	pub const fn new() -> Self {
		Self {
			data: ThinArrayPtr::empty(),
			length: 0,
			capacity: 0,
		}
	}

	pub fn with_capacity(capacity: u32) -> Self {
		Self {
			data: ThinArrayPtr::with_capacity(capacity as usize),
			length: 0,
			capacity,
		}
	}

	pub fn clear(&mut self) {
		let len = self.length;
		self.length = 0;
		// SAFETY: `len` is the length of the data stored within.
		unsafe { self.data.clear_elements(len as usize) };
	}

	pub fn as_ptr(&self) -> *mut T {
		self.data.as_ptr()
	}

	pub fn get(&self, index: u32) -> Option<&T> {
		(index < self.length).then(|| {
			// SAFETY: The check above ensures that the fetch is in bounds.
			unsafe { self.data.get_unchecked(index as usize) }
		})
	}

	pub fn get_mut(&mut self, index: u32) -> Option<&mut T> {
		(index < self.length).then(|| {
			// SAFETY: The check above ensures that the fetch is in bounds.
			unsafe { self.data.get_unchecked_mut(index as usize) }
		})
	}

	pub fn len(&self) -> u32 {
		self.length
	}

	pub fn capacity(&self) -> u32 {
		self.capacity
	}

	pub fn is_empty(&self) -> bool {
		self.length == 0
	}

	pub unsafe fn swap_remove_unchecked(&mut self, index: u32) -> T {
		self.data.swap_remove_unchecked(index as usize, self.length as usize)
	}

	pub fn push(&mut self, value: T) {
		self.reserve_for_push();
		unsafe { *self.data.get_unchecked_mut(self.length as usize) = value; }
		self.length = self.length.checked_add(1).unwrap();
	}

	pub unsafe fn push_unchecked(&mut self, value: T) {
		self.reserve_for_push();
		unsafe { *self.data.get_unchecked_mut(self.length as usize) = value; }
		self.length = self.length.checked_add(1).debug_checked_unwrap();
	}

	pub fn reserve(&mut self, additional: u32) {
		if self.length.checked_add(additional).unwrap() > self.capacity {
			if self.length == 0 {
				self.data.alloc(unsafe { NonZero::new_unchecked(1) });
				self.capacity = 1;
			} else {
				let new_capacity = self.capacity.next_power_of_two();
				unsafe { 
					self.data.realloc(
						NonZero::new_unchecked(self.capacity as usize),
						NonZero::new_unchecked(new_capacity as usize)
					);
				}
				self.capacity = new_capacity;
			}
		}
	}

	fn reserve_for_push(&mut self) {
		if self.length == self.capacity {
			if self.length == 0 {
				self.data.alloc(unsafe { NonZero::new_unchecked(1) });
				self.capacity = 1;
			} else {
				let new_capacity = self.capacity.next_power_of_two();
				unsafe { 
					self.data.realloc(
						NonZero::new_unchecked(self.capacity as usize),
						NonZero::new_unchecked(new_capacity as usize)
					);
				}
				self.capacity = new_capacity;
			}
		}
	}
}

impl<T: fmt::Debug> fmt::Debug for Vec32<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
	<[T] as fmt::Debug>::fmt(&*self, f)
    }
}

impl<T> Deref for Vec32<T> {
	type Target = [T];

	fn deref(&self) -> &Self::Target {
		unsafe { core::slice::from_raw_parts(self.as_ptr(), self.length as usize) }
	}
}

impl<T> DerefMut for Vec32<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { core::slice::from_raw_parts_mut(self.as_ptr(), self.length as usize) }
	}
}

impl<'a, T> IntoIterator for &'a Vec32<T> {
	type Item = &'a T;
	type IntoIter = core::slice::Iter<'a, T>;

	fn into_iter(self) -> Self::IntoIter {
	    <&[T] as IntoIterator>::into_iter(self.deref())
	}
}

impl<'a, T> IntoIterator for &'a mut Vec32<T> {
	type Item = &'a mut T;
	type IntoIter = core::slice::IterMut<'a, T>;

	fn into_iter(self) -> Self::IntoIter {
	    <&mut [T] as IntoIterator>::into_iter(self.deref_mut())
	}
}
 
impl<T> Default for Vec32<T> {
	fn default() -> Self {
		Self::new()
	}
}