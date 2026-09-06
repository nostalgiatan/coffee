//! Basic Memory Types
//!
//! Defines fundamental types for memory layout calculations in the Coffee compiler.
//! This module provides the core abstractions for representing memory layout
//! information, including alignment, size, and overall layout characteristics.
//! These types are used throughout the compiler's backend to ensure proper
//! memory layout and efficient memory usage.

use inkwell::types::BasicTypeEnum;

/// Memory alignment information
/// 
/// This structure represents memory alignment requirements for data types.
/// Alignment is a power-of-2 value that specifies the byte boundary on which
/// the data must be aligned in memory. Proper alignment is crucial for
/// performance and correctness on many architectures.
/// 
/// For example, a 4-byte integer typically requires 4-byte alignment, meaning
/// its address in memory must be a multiple of 4.
/// 
/// # Examples
/// 
/// ```
/// use coffee::backend::memory::types::Align;
/// 
/// // Create an alignment of 8 bytes
/// let align = Align::new(8);
/// assert_eq!(align.value, 8);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Align {
    /// The alignment value in bytes (must be a power of 2)
    pub value: u32,
}

impl Align {
    /// Create a new alignment with the specified value
    /// 
    /// # Arguments
    /// 
    /// * `value` - The alignment value in bytes, which must be a power of 2
    /// 
    /// # Panics
    /// 
    /// This function panics if the value is not a power of 2, as alignment
    /// values must be powers of 2 for proper memory management.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::types::Align;
    /// 
    /// let align = Align::new(8);
    /// assert_eq!(align.value, 8);
    /// 
    /// // This will panic because 6 is not a power of 2
    /// // let invalid_align = Align::new(6);
    /// ```
    pub fn new(value: u32) -> Self {
        assert!(value.is_power_of_two(), "Alignment must be power of 2");
        Self { value }
    }
}

/// Memory size information
/// 
/// This structure represents the size of data in memory, measured in bytes.
/// It provides utilities for calculating aligned sizes and determining
/// memory requirements for different types.
/// 
/// The Size type is essential for memory layout calculations and helps
/// ensure that data structures are properly sized and aligned in memory.
/// 
/// # Examples
/// 
/// ```
/// use coffee::backend::memory::types::Size;
/// 
/// let size = Size::new(16);
/// assert_eq!(size.bytes, 16);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Size {
    /// The size in bytes
    pub bytes: u64,
}

impl Size {
    /// Create a new size with the specified byte count
    /// 
    /// # Arguments
    /// 
    /// * `bytes` - The size in bytes
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::types::Size;
    /// 
    /// let size = Size::new(32);
    /// assert_eq!(size.bytes, 32);
    /// ```
    pub fn new(bytes: u64) -> Self {
        Self { bytes }
    }

    /// Align size up to alignment boundary
    /// 
    /// This function calculates the minimum size required to ensure that
    /// data with the given size is properly aligned according to the
    /// specified alignment requirement.
    /// 
    /// # Arguments
    /// 
    /// * `align` - The alignment boundary to align to
    /// 
    /// # Returns
    /// 
    /// The aligned size in bytes
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::types::{Size, Align};
    /// 
    /// let size = Size::new(13);
    /// let aligned = size.align_to(Align::new(8));
    /// assert_eq!(aligned, 16); // 13 aligned up to 8-byte boundary is 16
    /// ```
    pub fn align_to(self, align: Align) -> u64 {
        let mask = (align.value as u64) - 1;
        (self.bytes + mask) & !mask
    }
}

/// Complete memory layout information
/// 
/// This structure represents the complete memory layout of a data type,
/// combining both size and alignment information. Layout is a fundamental
/// concept in memory management that describes how data is arranged in
/// memory, including both its size and alignment requirements.
/// 
/// The Layout type is used throughout the compiler's memory management
/// system to ensure proper data placement and efficient memory usage.
/// It combines size (how much space is needed) and alignment (where
/// the data can be placed) information into a single, easy-to-use type.
/// 
/// # Examples
/// 
/// ```
/// use coffee::backend::memory::types::{Layout, Size, Align};
/// 
/// let layout = Layout::new(16, 8);
/// assert_eq!(layout.size.bytes, 16);
/// assert_eq!(layout.align.value, 8);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    /// The size of the layout in bytes
    pub size: Size,
    /// The alignment requirement of the layout
    pub align: Align,
}

impl Layout {
    /// Create a new layout with the specified size and alignment
    /// 
    /// # Arguments
    /// 
    /// * `size` - The size in bytes
    /// * `align` - The alignment requirement in bytes (must be a power of 2)
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::types::Layout;
    /// 
    /// let layout = Layout::new(32, 8);
    /// assert_eq!(layout.size.bytes, 32);
    /// assert_eq!(layout.align.value, 8);
    /// ```
    pub fn new(size: u64, align: u32) -> Self {
        Self {
            size: Size::new(size),
            align: Align::new(align),
        }
    }

    /// Calculate padding needed for alignment at given offset
    /// 
    /// This function determines how many bytes of padding are needed to
    /// ensure that data with this layout is properly aligned when placed
    /// at the specified offset.
    /// 
    /// # Arguments
    /// 
    /// * `offset` - The current offset where the data would be placed
    /// 
    /// # Returns
    /// 
    /// The number of padding bytes needed, or 0 if no padding is required
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::types::Layout;
    /// 
    /// let layout = Layout::new(8, 16); // 8 bytes, 16-byte aligned
    /// let padding = layout.padding_needed_for(4);
    /// assert_eq!(padding, 12); // Need 12 bytes of padding to reach 16-byte alignment
    /// 
    /// let padding = layout.padding_needed_for(16);
    /// assert_eq!(padding, 0); // Already aligned
    /// ```
    pub fn padding_needed_for(&self, offset: u64) -> u64 {
        let layout_align = self.align.value as u64;
        let misaligned = offset & (layout_align - 1);
        if misaligned != 0 {
            layout_align - misaligned
        } else {
            0
        }
    }

    /// Size and alignment from the module's LLVM target data (matches object layout).
    pub fn from_target_data(ty: &BasicTypeEnum, td: &inkwell::targets::TargetData) -> Self {
        let size = td.get_store_size(ty);
        let mut align = td.get_abi_alignment(ty);
        if align == 0 || !align.is_power_of_two() {
            align = 1;
        }
        Self::new(size, align)
    }
}
