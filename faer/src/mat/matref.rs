use super::*;
use crate::col::colref::ColRef;
use crate::internal_prelude::*;
use crate::row::rowref::RowRef;
use crate::utils::bound::{Dim, Partition};
use crate::{ContiguousFwd, Idx, IdxInc};
use core::ops::Index;
use equator::{assert, debug_assert};
use faer_traits::Real;
use generativity::Guard;
use matmut::MatMut;
use matown::Mat;

pub struct MatRef<'a, T, Rows = usize, Cols = usize, RStride = isize, CStride = isize> {
	pub(super) imp: MatView<T, Rows, Cols, RStride, CStride>,
	pub(super) __marker: PhantomData<&'a T>,
}

impl<T, Rows: Copy, Cols: Copy, RStride: Copy, CStride: Copy> Copy for MatRef<'_, T, Rows, Cols, RStride, CStride> {}
impl<T, Rows: Copy, Cols: Copy, RStride: Copy, CStride: Copy> Clone for MatRef<'_, T, Rows, Cols, RStride, CStride> {
	#[inline]
	fn clone(&self) -> Self {
		*self
	}
}

impl<'short, T, Rows: Copy, Cols: Copy, RStride: Copy, CStride: Copy> Reborrow<'short> for MatRef<'_, T, Rows, Cols, RStride, CStride> {
	type Target = MatRef<'short, T, Rows, Cols, RStride, CStride>;

	#[inline]
	fn rb(&'short self) -> Self::Target {
		*self
	}
}
impl<'short, T, Rows: Copy, Cols: Copy, RStride: Copy, CStride: Copy> ReborrowMut<'short> for MatRef<'_, T, Rows, Cols, RStride, CStride> {
	type Target = MatRef<'short, T, Rows, Cols, RStride, CStride>;

	#[inline]
	fn rb_mut(&'short mut self) -> Self::Target {
		*self
	}
}
impl<'a, T, Rows: Copy, Cols: Copy, RStride: Copy, CStride: Copy> IntoConst for MatRef<'a, T, Rows, Cols, RStride, CStride> {
	type Target = MatRef<'a, T, Rows, Cols, RStride, CStride>;

	#[inline]
	fn into_const(self) -> Self::Target {
		self
	}
}

unsafe impl<T: Sync, Rows: Sync, Cols: Sync, RStride: Sync, CStride: Sync> Sync for MatRef<'_, T, Rows, Cols, RStride, CStride> {}
unsafe impl<T: Sync, Rows: Send, Cols: Send, RStride: Send, CStride: Send> Send for MatRef<'_, T, Rows, Cols, RStride, CStride> {}

#[track_caller]
#[inline]
fn from_strided_column_major_slice_assert(nrows: usize, ncols: usize, col_stride: usize, len: usize) {
	if nrows > 0 && ncols > 0 {
		let last = usize::checked_mul(col_stride, ncols - 1).and_then(|last_col| last_col.checked_add(nrows - 1));
		let Some(last) = last else {
			panic!("address computation of the last matrix element overflowed");
		};
		assert!(last < len);
	}
}

impl<'a, T> MatRef<'a, T> {
	#[inline]
	pub fn from_row_major_array<const ROWS: usize, const COLS: usize>(array: &'a [[T; COLS]; ROWS]) -> Self {
		unsafe { Self::from_raw_parts(array as *const _ as *const T, ROWS, COLS, COLS as isize, 1) }
	}

	#[inline]
	pub fn from_column_major_array<const ROWS: usize, const COLS: usize>(array: &'a [[T; ROWS]; COLS]) -> Self {
		unsafe { Self::from_raw_parts(array as *const _ as *const T, ROWS, COLS, 1, ROWS as isize) }
	}

	#[inline]
	pub fn from_ref(value: &'a T) -> Self
	where
		T: Sized,
	{
		unsafe { MatRef::from_raw_parts(value as *const T, 1, 1, 0, 0) }
	}
}

impl<'a, T> MatRef<'a, T, Dim<'static>, Dim<'static>> {
	#[inline]
	pub fn from_ref_bound(value: &'a T) -> Self
	where
		T: Sized,
	{
		unsafe { MatRef::from_raw_parts(value as *const T, Dim::ONE, Dim::ONE, 0, 0) }
	}
}

impl<'a, T, Rows: Shape, Cols: Shape> MatRef<'a, T, Rows, Cols> {
	#[inline]
	pub fn from_repeated_ref(value: &'a T, nrows: Rows, ncols: Cols) -> Self
	where
		T: Sized,
	{
		unsafe { MatRef::from_raw_parts(value as *const T, nrows, ncols, 0, 0) }
	}

	#[inline]
	#[track_caller]
	pub fn from_column_major_slice(slice: &'a [T], nrows: Rows, ncols: Cols) -> Self
	where
		T: Sized,
	{
		from_slice_assert(nrows.unbound(), ncols.unbound(), slice.len());

		unsafe { MatRef::from_raw_parts(slice.as_ptr(), nrows, ncols, 1, nrows.unbound() as isize) }
	}

	#[inline]
	#[track_caller]
	pub fn from_column_major_slice_with_stride(slice: &'a [T], nrows: Rows, ncols: Cols, col_stride: usize) -> Self
	where
		T: Sized,
	{
		from_strided_column_major_slice_assert(nrows.unbound(), ncols.unbound(), col_stride, slice.len());

		unsafe { MatRef::from_raw_parts(slice.as_ptr(), nrows, ncols, 1, col_stride as isize) }
	}

	#[inline]
	#[track_caller]
	pub fn from_row_major_slice(slice: &'a [T], nrows: Rows, ncols: Cols) -> Self
	where
		T: Sized,
	{
		MatRef::from_column_major_slice(slice, ncols, nrows).transpose()
	}

	#[inline]
	#[track_caller]
	pub fn from_row_major_slice_with_stride(slice: &'a [T], nrows: Rows, ncols: Cols, row_stride: usize) -> Self
	where
		T: Sized,
	{
		MatRef::from_column_major_slice_with_stride(slice, ncols, nrows, row_stride).transpose()
	}
}

impl<'a, T, Rows: Shape, Cols: Shape, RStride: Stride, CStride: Stride> MatRef<'a, T, Rows, Cols, RStride, CStride> {
	#[inline]
	#[track_caller]
	pub unsafe fn from_raw_parts(ptr: *const T, nrows: Rows, ncols: Cols, row_stride: RStride, col_stride: CStride) -> Self {
		Self {
			imp: MatView {
				ptr: NonNull::new_unchecked(ptr as *mut T),
				nrows,
				ncols,
				row_stride,
				col_stride,
			},
			__marker: PhantomData,
		}
	}

	#[inline]
	pub fn as_ptr(&self) -> *const T {
		self.imp.ptr.as_ptr() as *const T
	}

	#[inline]
	pub fn nrows(&self) -> Rows {
		self.imp.nrows
	}

	#[inline]
	pub fn ncols(&self) -> Cols {
		self.imp.ncols
	}

	#[inline]
	pub fn shape(&self) -> (Rows, Cols) {
		(self.nrows(), self.ncols())
	}

	#[inline]
	pub fn row_stride(&self) -> RStride {
		self.imp.row_stride
	}

	#[inline]
	pub fn col_stride(&self) -> CStride {
		self.imp.col_stride
	}

	#[inline]
	pub fn ptr_at(&self, row: IdxInc<Rows>, col: IdxInc<Cols>) -> *const T {
		let ptr = self.as_ptr();

		if row >= self.nrows() || col >= self.ncols() {
			ptr
		} else {
			ptr.wrapping_offset(row.unbound() as isize * self.row_stride().element_stride())
				.wrapping_offset(col.unbound() as isize * self.col_stride().element_stride())
		}
	}

	#[inline]
	#[track_caller]
	pub unsafe fn ptr_inbounds_at(&self, row: Idx<Rows>, col: Idx<Cols>) -> *const T {
		debug_assert!(all(row < self.nrows(), col < self.ncols()));
		self.as_ptr()
			.offset(row.unbound() as isize * self.row_stride().element_stride())
			.offset(col.unbound() as isize * self.col_stride().element_stride())
	}

	#[inline]
	#[track_caller]
	pub fn split_at(
		self,
		row: IdxInc<Rows>,
		col: IdxInc<Cols>,
	) -> (
		MatRef<'a, T, usize, usize, RStride, CStride>,
		MatRef<'a, T, usize, usize, RStride, CStride>,
		MatRef<'a, T, usize, usize, RStride, CStride>,
		MatRef<'a, T, usize, usize, RStride, CStride>,
	) {
		assert!(all(row <= self.nrows(), col <= self.ncols()));

		let rs = self.row_stride();
		let cs = self.col_stride();

		let top_left = self.ptr_at(Rows::start(), Cols::start());
		let top_right = self.ptr_at(Rows::start(), col);
		let bot_left = self.ptr_at(row, Cols::start());
		let bot_right = self.ptr_at(row, col);

		unsafe {
			(
				MatRef::from_raw_parts(top_left, row.unbound(), col.unbound(), rs, cs),
				MatRef::from_raw_parts(top_right, row.unbound(), self.ncols().unbound() - col.unbound(), rs, cs),
				MatRef::from_raw_parts(bot_left, self.nrows().unbound() - row.unbound(), col.unbound(), rs, cs),
				MatRef::from_raw_parts(bot_right, self.nrows().unbound() - row.unbound(), self.ncols().unbound() - col.unbound(), rs, cs),
			)
		}
	}

	#[inline]
	#[track_caller]
	pub fn split_at_row(self, row: IdxInc<Rows>) -> (MatRef<'a, T, usize, Cols, RStride, CStride>, MatRef<'a, T, usize, Cols, RStride, CStride>) {
		assert!(all(row <= self.nrows()));

		let rs = self.row_stride();
		let cs = self.col_stride();

		let top = self.ptr_at(Rows::start(), Cols::start());
		let bot = self.ptr_at(row, Cols::start());

		unsafe {
			(
				MatRef::from_raw_parts(top, row.unbound(), self.ncols(), rs, cs),
				MatRef::from_raw_parts(bot, self.nrows().unbound() - row.unbound(), self.ncols(), rs, cs),
			)
		}
	}

	#[inline]
	#[track_caller]
	pub fn split_at_col(self, col: IdxInc<Cols>) -> (MatRef<'a, T, Rows, usize, RStride, CStride>, MatRef<'a, T, Rows, usize, RStride, CStride>) {
		assert!(all(col <= self.ncols()));

		let rs = self.row_stride();
		let cs = self.col_stride();

		let left = self.ptr_at(Rows::start(), Cols::start());
		let right = self.ptr_at(Rows::start(), col);

		unsafe {
			(
				MatRef::from_raw_parts(left, self.nrows(), col.unbound(), rs, cs),
				MatRef::from_raw_parts(right, self.nrows(), self.ncols().unbound() - col.unbound(), rs, cs),
			)
		}
	}

	#[inline]
	pub fn transpose(self) -> MatRef<'a, T, Cols, Rows, CStride, RStride> {
		MatRef {
			imp: MatView {
				ptr: self.imp.ptr,
				nrows: self.imp.ncols,
				ncols: self.imp.nrows,
				row_stride: self.imp.col_stride,
				col_stride: self.imp.row_stride,
			},
			__marker: PhantomData,
		}
	}

	#[inline]
	pub fn conjugate(self) -> MatRef<'a, T::Conj, Rows, Cols, RStride, CStride>
	where
		T: Conjugate,
	{
		unsafe { MatRef::from_raw_parts(self.as_ptr() as *const T::Conj, self.nrows(), self.ncols(), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn canonical(self) -> MatRef<'a, T::Canonical, Rows, Cols, RStride, CStride>
	where
		T: Conjugate,
	{
		unsafe { MatRef::from_raw_parts(self.as_ptr() as *const T::Canonical, self.nrows(), self.ncols(), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn adjoint(self) -> MatRef<'a, T::Conj, Cols, Rows, CStride, RStride>
	where
		T: Conjugate,
	{
		self.conjugate().transpose()
	}

	#[inline]
	#[track_caller]
	pub(crate) fn at(self, row: Idx<Rows>, col: Idx<Cols>) -> &'a T {
		assert!(all(row < self.nrows(), col < self.ncols()));
		unsafe { self.at_unchecked(row, col) }
	}

	#[inline]
	#[track_caller]
	pub(crate) fn read(&self, row: Idx<Rows>, col: Idx<Cols>) -> T
	where
		T: Clone,
	{
		self.at(row, col).clone()
	}

	#[inline]
	#[track_caller]
	pub(crate) unsafe fn at_unchecked(self, row: Idx<Rows>, col: Idx<Cols>) -> &'a T {
		&*self.ptr_inbounds_at(row, col)
	}

	#[inline]
	pub fn reverse_rows(self) -> MatRef<'a, T, Rows, Cols, RStride::Rev, CStride> {
		let row = unsafe { IdxInc::<Rows>::new_unbound(self.nrows().unbound().saturating_sub(1)) };
		let ptr = self.ptr_at(row, Cols::start());
		unsafe { MatRef::from_raw_parts(ptr, self.nrows(), self.ncols(), self.row_stride().rev(), self.col_stride()) }
	}

	#[inline]
	pub fn reverse_cols(self) -> MatRef<'a, T, Rows, Cols, RStride, CStride::Rev> {
		let col = unsafe { IdxInc::<Cols>::new_unbound(self.ncols().unbound().saturating_sub(1)) };
		let ptr = self.ptr_at(Rows::start(), col);
		unsafe { MatRef::from_raw_parts(ptr, self.nrows(), self.ncols(), self.row_stride(), self.col_stride().rev()) }
	}

	#[inline]
	pub fn reverse_rows_and_cols(self) -> MatRef<'a, T, Rows, Cols, RStride::Rev, CStride::Rev> {
		self.reverse_rows().reverse_cols()
	}

	#[inline]
	#[track_caller]
	pub fn submatrix<V: Shape, H: Shape>(self, row_start: IdxInc<Rows>, col_start: IdxInc<Cols>, nrows: V, ncols: H) -> MatRef<'a, T, V, H, RStride, CStride> {
		assert!(all(row_start <= self.nrows(), col_start <= self.ncols()));
		{
			let nrows = nrows.unbound();
			let full_nrows = self.nrows().unbound();
			let row_start = row_start.unbound();
			let ncols = ncols.unbound();
			let full_ncols = self.ncols().unbound();
			let col_start = col_start.unbound();
			assert!(all(nrows <= full_nrows - row_start, ncols <= full_ncols - col_start,));
		}
		let rs = self.row_stride();
		let cs = self.col_stride();

		unsafe { MatRef::from_raw_parts(self.ptr_at(row_start, col_start), nrows, ncols, rs, cs) }
	}

	#[inline]
	#[track_caller]
	pub fn subrows<V: Shape>(self, row_start: IdxInc<Rows>, nrows: V) -> MatRef<'a, T, V, Cols, RStride, CStride> {
		assert!(all(row_start <= self.nrows()));
		{
			let nrows = nrows.unbound();
			let full_nrows = self.nrows().unbound();
			let row_start = row_start.unbound();
			assert!(all(nrows <= full_nrows - row_start));
		}
		let rs = self.row_stride();
		let cs = self.col_stride();

		unsafe { MatRef::from_raw_parts(self.ptr_at(row_start, Cols::start()), nrows, self.ncols(), rs, cs) }
	}

	#[inline]
	#[track_caller]
	pub fn subcols<H: Shape>(self, col_start: IdxInc<Cols>, ncols: H) -> MatRef<'a, T, Rows, H, RStride, CStride> {
		assert!(all(col_start <= self.ncols()));
		{
			let ncols = ncols.unbound();
			let full_ncols = self.ncols().unbound();
			let col_start = col_start.unbound();
			assert!(all(ncols <= full_ncols - col_start));
		}
		let rs = self.row_stride();
		let cs = self.col_stride();

		unsafe { MatRef::from_raw_parts(self.ptr_at(Rows::start(), col_start), self.nrows(), ncols, rs, cs) }
	}

	#[inline]
	#[track_caller]
	pub fn as_shape<V: Shape, H: Shape>(self, nrows: V, ncols: H) -> MatRef<'a, T, V, H, RStride, CStride> {
		assert!(all(self.nrows().unbound() == nrows.unbound(), self.ncols().unbound() == ncols.unbound(),));
		unsafe { MatRef::from_raw_parts(self.as_ptr(), nrows, ncols, self.row_stride(), self.col_stride()) }
	}

	#[inline]
	#[track_caller]
	pub fn as_row_shape<V: Shape>(self, nrows: V) -> MatRef<'a, T, V, Cols, RStride, CStride> {
		assert!(all(self.nrows().unbound() == nrows.unbound()));
		unsafe { MatRef::from_raw_parts(self.as_ptr(), nrows, self.ncols(), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	#[track_caller]
	pub fn as_col_shape<H: Shape>(self, ncols: H) -> MatRef<'a, T, Rows, H, RStride, CStride> {
		assert!(all(self.ncols().unbound() == ncols.unbound()));
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows(), ncols, self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn as_dyn_stride(self) -> MatRef<'a, T, Rows, Cols, isize, isize> {
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows(), self.ncols(), self.row_stride().element_stride(), self.col_stride().element_stride()) }
	}

	#[inline]
	pub fn as_dyn(self) -> MatRef<'a, T, usize, usize, RStride, CStride> {
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows().unbound(), self.ncols().unbound(), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn as_dyn_rows(self) -> MatRef<'a, T, usize, Cols, RStride, CStride> {
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows().unbound(), self.ncols(), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn as_dyn_cols(self) -> MatRef<'a, T, Rows, usize, RStride, CStride> {
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows(), self.ncols().unbound(), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	#[track_caller]
	pub fn row(self, i: Idx<Rows>) -> RowRef<'a, T, Cols, CStride> {
		assert!(i < self.nrows());

		unsafe { RowRef::from_raw_parts(self.ptr_at(i.into(), Cols::start()), self.ncols(), self.col_stride()) }
	}

	#[inline]
	#[track_caller]
	pub fn col(self, j: Idx<Cols>) -> ColRef<'a, T, Rows, RStride> {
		assert!(j < self.ncols());

		unsafe { ColRef::from_raw_parts(self.ptr_at(Rows::start(), j.into()), self.nrows(), self.row_stride()) }
	}

	#[inline]
	pub fn col_iter(self) -> impl 'a + ExactSizeIterator + DoubleEndedIterator<Item = ColRef<'a, T, Rows, RStride>>
	where
		Rows: 'a,
		Cols: 'a,
	{
		Cols::indices(Cols::start(), self.ncols().end()).map(move |j| self.col(j))
	}

	#[inline]
	pub fn row_iter(self) -> impl 'a + ExactSizeIterator + DoubleEndedIterator<Item = RowRef<'a, T, Cols, CStride>>
	where
		Rows: 'a,
		Cols: 'a,
	{
		Rows::indices(Rows::start(), self.nrows().end()).map(move |i| self.row(i))
	}

	#[inline]
	#[cfg(feature = "rayon")]
	pub fn par_col_iter(self) -> impl 'a + rayon::iter::IndexedParallelIterator<Item = ColRef<'a, T, Rows, RStride>>
	where
		T: Sync,
		Rows: 'a,
		Cols: 'a,
	{
		use rayon::prelude::*;

		#[inline]
		fn col_fn<T, Rows: Shape, RStride: Stride, CStride: Stride>(col: MatRef<'_, T, Rows, usize, RStride, CStride>) -> ColRef<'_, T, Rows, RStride> {
			col.col(0)
		}

		self.par_col_chunks(1).map(col_fn)
	}

	#[inline]
	#[cfg(feature = "rayon")]
	pub fn par_row_iter(self) -> impl 'a + rayon::iter::IndexedParallelIterator<Item = RowRef<'a, T, Cols, CStride>>
	where
		T: Sync,
		Rows: 'a,
		Cols: 'a,
	{
		use rayon::prelude::*;
		self.transpose().par_col_iter().map(ColRef::transpose)
	}

	#[inline]
	#[track_caller]
	#[cfg(feature = "rayon")]
	pub fn par_col_chunks(self, chunk_size: usize) -> impl 'a + rayon::iter::IndexedParallelIterator<Item = MatRef<'a, T, Rows, usize, RStride, CStride>>
	where
		T: Sync,
		Rows: 'a,
		Cols: 'a,
	{
		use rayon::prelude::*;

		let this = self.as_dyn_cols();

		assert!(chunk_size > 0);
		let chunk_count = this.ncols().div_ceil(chunk_size);
		(0..chunk_count).into_par_iter().map(move |chunk_idx| {
			let pos = chunk_size * chunk_idx;
			this.subcols(pos, Ord::min(chunk_size, this.ncols() - pos))
		})
	}

	#[inline]
	#[track_caller]
	#[cfg(feature = "rayon")]
	pub fn par_col_partition(self, count: usize) -> impl 'a + rayon::iter::IndexedParallelIterator<Item = MatRef<'a, T, Rows, usize, RStride, CStride>>
	where
		T: Sync,
		Rows: 'a,
		Cols: 'a,
	{
		use rayon::prelude::*;

		let this = self.as_dyn_cols();

		assert!(count > 0);
		(0..count).into_par_iter().map(move |chunk_idx| {
			let (start, len) = crate::utils::thread::par_split_indices(this.ncols(), chunk_idx, count);
			this.subcols(start, len)
		})
	}

	#[inline]
	#[track_caller]
	#[cfg(feature = "rayon")]
	pub fn par_row_chunks(self, chunk_size: usize) -> impl 'a + rayon::iter::IndexedParallelIterator<Item = MatRef<'a, T, usize, Cols, RStride, CStride>>
	where
		T: Sync,
		Rows: 'a,
		Cols: 'a,
	{
		use rayon::prelude::*;
		self.transpose().par_col_chunks(chunk_size).map(MatRef::transpose)
	}

	#[inline]
	#[track_caller]
	#[cfg(feature = "rayon")]
	pub fn par_row_partition(self, count: usize) -> impl 'a + rayon::iter::IndexedParallelIterator<Item = MatRef<'a, T, usize, Cols, RStride, CStride>>
	where
		T: Sync,
		Rows: 'a,
		Cols: 'a,
	{
		use rayon::prelude::*;
		self.transpose().par_col_partition(count).map(MatRef::transpose)
	}

	#[inline]
	pub fn split_first_row(self) -> Option<(RowRef<'a, T, Cols, CStride>, MatRef<'a, T, usize, Cols, RStride, CStride>)> {
		if let Some(i0) = self.nrows().idx_inc(1) {
			let (head, tail) = self.split_at_row(i0);
			Some((head.row(0), tail))
		} else {
			None
		}
	}

	#[inline]
	pub fn split_first_col(self) -> Option<(ColRef<'a, T, Rows, RStride>, MatRef<'a, T, Rows, usize, RStride, CStride>)> {
		if let Some(i0) = self.ncols().idx_inc(1) {
			let (head, tail) = self.split_at_col(i0);
			Some((head.col(0), tail))
		} else {
			None
		}
	}

	#[inline]
	pub fn split_last_row(self) -> Option<(RowRef<'a, T, Cols, CStride>, MatRef<'a, T, usize, Cols, RStride, CStride>)> {
		if self.nrows().unbound() > 0 {
			let i0 = self.nrows().checked_idx_inc(self.nrows().unbound() - 1);
			let (head, tail) = self.split_at_row(i0);
			Some((tail.row(0), head))
		} else {
			None
		}
	}

	#[inline]
	pub fn split_last_col(self) -> Option<(ColRef<'a, T, Rows, RStride>, MatRef<'a, T, Rows, usize, RStride, CStride>)> {
		if self.ncols().unbound() > 0 {
			let i0 = self.ncols().checked_idx_inc(self.ncols().unbound() - 1);
			let (head, tail) = self.split_at_col(i0);
			Some((tail.col(0), head))
		} else {
			None
		}
	}

	#[inline]
	pub fn cloned(self) -> Mat<T, Rows, Cols>
	where
		T: Clone,
	{
		fn imp<'M, 'N, T: Clone, RStride: Stride, CStride: Stride>(this: MatRef<'_, T, Dim<'M>, Dim<'N>, RStride, CStride>) -> Mat<T, Dim<'M>, Dim<'N>> {
			Mat::from_fn(this.nrows(), this.ncols(), |i, j| this.at(i, j).clone())
		}

		with_dim!(M, self.nrows().unbound());
		with_dim!(N, self.ncols().unbound());
		imp(self.as_shape(M, N)).into_shape(self.nrows(), self.ncols())
	}

	#[inline]
	pub fn to_owned(self) -> Mat<T::Canonical, Rows, Cols>
	where
		T: Conjugate,
	{
		fn imp<'M, 'N, T, RStride: Stride, CStride: Stride>(this: MatRef<'_, T, Dim<'M>, Dim<'N>, RStride, CStride>) -> Mat<T::Canonical, Dim<'M>, Dim<'N>>
		where
			T: Conjugate,
		{
			Mat::from_fn(this.nrows(), this.ncols(), |i, j| Conj::apply::<T>(this.at(i, j)))
		}

		with_dim!(M, self.nrows().unbound());
		with_dim!(N, self.ncols().unbound());
		imp(self.as_shape(M, N)).into_shape(self.nrows(), self.ncols())
	}

	#[inline]
	pub unsafe fn const_cast(self) -> MatMut<'a, T, Rows, Cols, RStride, CStride> {
		MatMut::from_raw_parts_mut(self.as_ptr() as *mut T, self.nrows(), self.ncols(), self.row_stride(), self.col_stride())
	}

	#[inline]
	pub fn try_as_col_major(self) -> Option<MatRef<'a, T, Rows, Cols, ContiguousFwd, CStride>> {
		if self.row_stride().element_stride() == 1 {
			Some(unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows(), self.ncols(), ContiguousFwd, self.col_stride()) })
		} else {
			None
		}
	}

	#[inline]
	pub fn try_as_row_major(self) -> Option<MatRef<'a, T, Rows, Cols, RStride, ContiguousFwd>> {
		if self.col_stride().element_stride() == 1 {
			Some(unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows(), self.ncols(), self.row_stride(), ContiguousFwd) })
		} else {
			None
		}
	}

	#[inline]
	pub fn as_ref(&self) -> MatRef<'_, T, Rows, Cols, RStride, CStride> {
		*self
	}

	#[inline]
	pub fn bind<'M, 'N>(self, row: Guard<'M>, col: Guard<'N>) -> MatRef<'a, T, Dim<'M>, Dim<'N>, RStride, CStride> {
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows().bind(row), self.ncols().bind(col), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn bind_r<'M>(self, row: Guard<'M>) -> MatRef<'a, T, Dim<'M>, Cols, RStride, CStride> {
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows().bind(row), self.ncols(), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn bind_c<'N>(self, col: Guard<'N>) -> MatRef<'a, T, Rows, Dim<'N>, RStride, CStride> {
		unsafe { MatRef::from_raw_parts(self.as_ptr(), self.nrows(), self.ncols().bind(col), self.row_stride(), self.col_stride()) }
	}

	#[inline]
	pub fn norm_max(&self) -> Real<T>
	where
		T: Conjugate,
	{
		linalg::reductions::norm_max::norm_max(self.canonical().as_dyn_stride().as_dyn())
	}

	#[inline]
	pub fn norm_l2(&self) -> Real<T>
	where
		T: Conjugate,
	{
		linalg::reductions::norm_l2::norm_l2(self.canonical().as_dyn_stride().as_dyn())
	}

	#[inline]
	pub fn squared_norm_l2(&self) -> Real<T>
	where
		T: Conjugate,
	{
		linalg::reductions::norm_l2_sqr::norm_l2_sqr(self.canonical().as_dyn_stride().as_dyn())
	}

	#[inline]
	pub fn norm_l1(&self) -> Real<T>
	where
		T: Conjugate,
	{
		linalg::reductions::norm_l1::norm_l1(self.canonical().as_dyn_stride().as_dyn())
	}

	#[inline]
	#[math]
	pub fn sum(&self) -> T::Canonical
	where
		T: Conjugate,
	{
		let val = linalg::reductions::sum::sum(self.canonical().as_dyn_stride().as_dyn());
		if const { Conj::get::<T>().is_conj() } { conj(val) } else { val }
	}

	#[track_caller]
	#[inline]
	pub fn get<RowRange, ColRange>(self, row: RowRange, col: ColRange) -> <MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<RowRange, ColRange>>::Target
	where
		MatRef<'a, T, Rows, Cols, RStride, CStride>: MatIndex<RowRange, ColRange>,
	{
		<MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<RowRange, ColRange>>::get(self, row, col)
	}

	#[track_caller]
	#[inline]
	pub fn get_r<RowRange>(self, row: RowRange) -> <MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<RowRange, core::ops::RangeFull>>::Target
	where
		MatRef<'a, T, Rows, Cols, RStride, CStride>: MatIndex<RowRange, core::ops::RangeFull>,
	{
		<MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<RowRange, core::ops::RangeFull>>::get(self, row, ..)
	}

	#[track_caller]
	#[inline]
	pub fn get_c<ColRange>(self, col: ColRange) -> <MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<core::ops::RangeFull, ColRange>>::Target
	where
		MatRef<'a, T, Rows, Cols, RStride, CStride>: MatIndex<core::ops::RangeFull, ColRange>,
	{
		<MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<core::ops::RangeFull, ColRange>>::get(self, .., col)
	}

	#[track_caller]
	#[inline]
	pub unsafe fn get_unchecked<RowRange, ColRange>(self, row: RowRange, col: ColRange) -> <MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<RowRange, ColRange>>::Target
	where
		MatRef<'a, T, Rows, Cols, RStride, CStride>: MatIndex<RowRange, ColRange>,
	{
		unsafe { <MatRef<'a, T, Rows, Cols, RStride, CStride> as MatIndex<RowRange, ColRange>>::get_unchecked(self, row, col) }
	}

	#[inline]
	pub(crate) fn __at(self, (i, j): (Idx<Rows>, Idx<Cols>)) -> &'a T {
		self.at(i, j)
	}
}

impl<'a, T, Dim: Shape, RStride: Stride, CStride: Stride> MatRef<'a, T, Dim, Dim, RStride, CStride> {
	#[inline]
	pub fn diagonal(self) -> DiagRef<'a, T, Dim, isize> {
		let k = Ord::min(self.nrows(), self.ncols());
		DiagRef {
			inner: unsafe { ColRef::from_raw_parts(self.as_ptr(), k, self.row_stride().element_stride() + self.col_stride().element_stride()) },
		}
	}
}

impl<'ROWS, 'COLS, 'a, T, RStride: Stride, CStride: Stride> MatRef<'a, T, Dim<'ROWS>, Dim<'COLS>, RStride, CStride> {
	#[inline]
	pub fn split_with<'TOP, 'BOT, 'LEFT, 'RIGHT>(
		self,
		row: Partition<'TOP, 'BOT, 'ROWS>,
		col: Partition<'LEFT, 'RIGHT, 'COLS>,
	) -> (
		MatRef<'a, T, Dim<'TOP>, Dim<'LEFT>, RStride, CStride>,
		MatRef<'a, T, Dim<'TOP>, Dim<'RIGHT>, RStride, CStride>,
		MatRef<'a, T, Dim<'BOT>, Dim<'LEFT>, RStride, CStride>,
		MatRef<'a, T, Dim<'BOT>, Dim<'RIGHT>, RStride, CStride>,
	) {
		let (a, b, c, d) = self.split_at(row.midpoint(), col.midpoint());
		(
			a.as_shape(row.head, col.head),
			b.as_shape(row.head, col.tail),
			c.as_shape(row.tail, col.head),
			d.as_shape(row.tail, col.tail),
		)
	}
}

impl<'ROWS, 'a, T, Cols: Shape, RStride: Stride, CStride: Stride> MatRef<'a, T, Dim<'ROWS>, Cols, RStride, CStride> {
	#[inline]
	pub fn split_rows_with<'TOP, 'BOT>(self, row: Partition<'TOP, 'BOT, 'ROWS>) -> (MatRef<'a, T, Dim<'TOP>, Cols, RStride, CStride>, MatRef<'a, T, Dim<'BOT>, Cols, RStride, CStride>) {
		let (a, b) = self.split_at_row(row.midpoint());
		(a.as_row_shape(row.head), b.as_row_shape(row.tail))
	}
}

impl<'COLS, 'a, T, Rows: Shape, RStride: Stride, CStride: Stride> MatRef<'a, T, Rows, Dim<'COLS>, RStride, CStride> {
	#[inline]
	pub fn split_cols_with<'LEFT, 'RIGHT>(self, col: Partition<'LEFT, 'RIGHT, 'COLS>) -> (MatRef<'a, T, Rows, Dim<'LEFT>, RStride, CStride>, MatRef<'a, T, Rows, Dim<'RIGHT>, RStride, CStride>) {
		let (a, b) = self.split_at_col(col.midpoint());
		(a.as_col_shape(col.head), b.as_col_shape(col.tail))
	}
}

impl<T, Rows: Shape, Cols: Shape, RStride: Stride, CStride: Stride> Index<(Idx<Rows>, Idx<Cols>)> for MatRef<'_, T, Rows, Cols, RStride, CStride> {
	type Output = T;

	#[inline]
	#[track_caller]
	fn index(&self, (row, col): (Idx<Rows>, Idx<Cols>)) -> &Self::Output {
		self.at(row, col)
	}
}

impl<'a, T: core::fmt::Debug, Rows: Shape, Cols: Shape, RStride: Stride, CStride: Stride> core::fmt::Debug for MatRef<'a, T, Rows, Cols, RStride, CStride> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		fn imp<'M, 'N, T: core::fmt::Debug>(this: MatRef<'_, T, Dim<'M>, Dim<'N>>, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
			writeln!(f, "[")?;
			for i in this.nrows().indices() {
				this.row(i).fmt(f)?;
				f.write_str(",\n")?;
			}
			write!(f, "]")
		}

		with_dim!(M, self.nrows().unbound());
		with_dim!(N, self.ncols().unbound());
		imp(self.as_shape(M, N).as_dyn_stride(), f)
	}
}

impl<'a, T, Rows: Shape, Cols: Shape, Rs: Stride, Cs: Stride, RowRange, ColRange> azucar::Index<'a, (RowRange, ColRange)> for MatRef<'_, T, Rows, Cols, Rs, Cs>
where
	MatRef<'a, T, Rows, Cols, Rs, Cs>: MatIndex<RowRange, ColRange>,
{
	type Output = <MatRef<'a, T, Rows, Cols, Rs, Cs> as MatIndex<RowRange, ColRange>>::Target;

	#[inline]
	fn index(&'a self, (row, col): (RowRange, ColRange)) -> Self::Output {
		<MatRef<'a, T, Rows, Cols, Rs, Cs> as MatIndex<RowRange, ColRange>>::get(self.as_ref(), row, col)
	}
}

impl<'a, T, Rows: Shape, Cols: Shape, Rs: Stride, Cs: Stride, RowRange, ColRange> azucar::IndexMut<'a, (RowRange, ColRange)> for MatRef<'_, T, Rows, Cols, Rs, Cs>
where
	MatRef<'a, T, Rows, Cols, Rs, Cs>: MatIndex<RowRange, ColRange>,
{
	type Output = <MatRef<'a, T, Rows, Cols, Rs, Cs> as MatIndex<RowRange, ColRange>>::Target;

	#[inline]
	fn index_mut(&'a mut self, (row, col): (RowRange, ColRange)) -> Self::Output {
		<MatRef<'a, T, Rows, Cols, Rs, Cs> as MatIndex<RowRange, ColRange>>::get(self.as_ref(), row, col)
	}
}

impl<'a, T, Rows: Shape, Cols: Shape, Rs: Stride, Cs: Stride, RowRange, ColRange> azucar::IndexMove<(RowRange, ColRange)> for MatRef<'a, T, Rows, Cols, Rs, Cs>
where
	MatRef<'a, T, Rows, Cols, Rs, Cs>: MatIndex<RowRange, ColRange>,
{
	type Output = <MatRef<'a, T, Rows, Cols, Rs, Cs> as MatIndex<RowRange, ColRange>>::Target;

	#[inline]
	fn index_move(self, (row, col): (RowRange, ColRange)) -> Self::Output {
		<MatRef<'a, T, Rows, Cols, Rs, Cs> as MatIndex<RowRange, ColRange>>::get(self, row, col)
	}
}
