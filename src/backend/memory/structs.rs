//! Struct Layout Optimization
//! 
//! Provides optimal field ordering and padding calculation for structs in the Coffee compiler.
//! This module implements sophisticated algorithms to minimize memory usage by optimizing
//! the layout of struct fields. It includes automatic field reordering to reduce padding
//! and memory efficiency analysis to help developers understand memory usage patterns.

use super::types::{Layout, Size, Align};

/// Field layout information
/// 
/// This structure represents the layout of a single field within a struct,
/// including its position (offset), memory requirements (layout), and
/// original ordering information. FieldLayout is used by the struct layout
/// optimization algorithms to determine the most memory-efficient arrangement
/// of fields within a struct.
/// 
/// The field layout optimization is crucial for performance, as it can
/// significantly reduce memory usage by minimizing padding between fields.
#[derive(Debug, Clone)]
pub struct FieldLayout {
    /// The offset of the field in bytes from the start of the struct
    pub offset: u64,
    /// The memory layout requirements of the field (size and alignment)
    pub layout: Layout,
    /// Original field index before reordering
    /// This is used to map back to the original field definition order
    pub original_index: usize,
}

/// Struct layout with field offsets and padding
/// 
/// This structure represents the complete memory layout of a struct, including
/// the positioning of each field, the total size of the struct, its alignment
/// requirements, and the amount of padding used. StructLayout is the core
/// component of the Coffee compiler's memory optimization system, enabling
/// efficient struct layouts that minimize memory usage.
/// 
/// The struct layout optimization algorithm reorders fields to minimize
/// padding while maintaining proper alignment requirements. This can result
/// in significant memory savings, especially for large structs with many fields
/// of varying sizes.
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// Vector of field layouts, each containing offset and layout information
    pub fields: Vec<FieldLayout>,
    /// Total size of the struct in bytes (including padding)
    pub size: u64,
    /// Alignment requirement of the struct in bytes
    pub align: u32,
    /// Total amount of padding in the struct (between fields and tail padding)
    #[allow(dead_code)]
    pub padding: u64,
}

impl StructLayout {
    /// Calculate optimal layout for struct fields
    /// 
    /// This function calculates an optimized memory layout for the given field layouts.
    /// It uses field reordering to minimize padding and maximize memory efficiency.
    /// 
    /// By default, this function enables field reordering (to minimize padding) and
    /// disables packed mode (to maintain proper alignment).
    /// 
    /// # Arguments
    /// 
    /// * `field_layouts` - Slice of Layout instances representing each field
    /// 
    /// # Returns
    /// 
    /// A StructLayout instance with optimized field placement
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::structs::StructLayout;
    /// use coffee::backend::memory::types::Layout;
    /// 
    /// // Create layouts for fields of different sizes
    /// let layouts = vec![
    ///     Layout::new(1, 1),   // 1 byte field (char/bool)
    ///     Layout::new(8, 8),   // 8 byte field (pointer/double)
    ///     Layout::new(4, 4),   // 4 byte field (int/float)
    /// ];
    /// 
    /// let layout = StructLayout::calculate(&layouts);
    /// // The fields may be reordered to minimize padding
    /// ```
    pub fn calculate(field_layouts: &[Layout]) -> Self {
        Self::calculate_with_options(field_layouts, true, false)
    }

    /// Calculate layout with packed option (no padding)
    /// 
    /// This function calculates a packed memory layout where all padding is removed.
    /// In packed mode, fields are placed directly adjacent to each other without
    /// any padding for alignment. This can save memory but may result in
    /// unaligned memory accesses that are slower on some architectures.
    /// 
    /// # Arguments
    /// 
    /// * `field_layouts` - Slice of Layout instances representing each field
    /// 
    /// # Returns
    /// 
    /// A StructLayout instance with packed layout (no padding)
    pub fn calculate_packed(field_layouts: &[Layout]) -> Self {
        Self::calculate_with_options(field_layouts, false, true)
    }

    /// Calculate layout with optional optimizations
    /// 
    /// This is the main function for calculating struct layouts with configurable
    /// optimization options. It allows fine-tuning of the layout process based
    /// on specific requirements.
    /// 
    /// # Arguments
    /// 
    /// * `field_layouts` - Slice of Layout instances representing each field
    /// * `reorder_fields` - If true, reorder fields to minimize padding (useful for memory efficiency)
    /// * `packed` - If true, disable all padding (useful for C compatibility or minimal size)
    /// 
    /// # Returns
    /// 
    /// A StructLayout instance configured according to the specified options
    pub fn calculate_with_options(field_layouts: &[Layout], reorder_fields: bool, packed: bool) -> Self {
        let mut fields = Vec::new();
        let mut offset = 0u64;
        let mut max_align = 1u32;

        // Optionally reorder fields to minimize padding
        let layouts = if reorder_fields && !packed {
            let mut sorted: Vec<(usize, Layout)> = field_layouts.iter().enumerate().map(|(i, &l)| (i, l)).collect();
            // Sort by alignment (descending), then by size (descending)
            // This places fields with higher alignment requirements first
            sorted.sort_by(|a, b| {
                b.1.align.value.cmp(&a.1.align.value)
                    .then_with(|| b.1.size.bytes.cmp(&a.1.size.bytes))
            });
            sorted
        } else {
            field_layouts.iter().enumerate().map(|(i, &l)| (i, l)).collect()
        };

        for (original_idx, field_layout) in layouts.iter() {
            // Update max alignment (unless packed)
            if !packed {
                max_align = max_align.max(field_layout.align.value);
            }

            // Calculate padding needed before this field (skip if packed)
            let padding = if packed {
                0
            } else {
                field_layout.padding_needed_for(offset)
            };
            offset += padding;

            // Record field layout with original index
            fields.push(FieldLayout {
                offset,
                layout: *field_layout,
                original_index: *original_idx,
            });

            // Move offset past this field
            offset += field_layout.size.bytes;
        }

        // Add tail padding to align struct size (skip if packed)
        let (size, padding) = if packed {
            (offset, 0)
        } else {
            let size_obj = Size::new(offset);
            let aligned_size = size_obj.align_to(Align::new(max_align));
            let tail_padding = aligned_size - offset;
            (aligned_size, tail_padding)
        };

        Self {
            fields,
            size,
            align: if packed { 1 } else { max_align },
            padding,
        }
    }

    /// Calculate memory efficiency metrics
    /// 
    /// This function computes important metrics about the memory efficiency
    /// of the struct layout, including the amount of actual data versus
    /// padding, and the overall efficiency percentage.
    /// 
    /// The efficiency metrics help developers understand how effectively
    /// the struct is using memory and whether optimization is beneficial.
    /// 
    /// # Returns
    /// 
    /// A tuple containing:
    /// - `u64`: The number of bytes containing actual data (sum of field sizes)
    /// - `u64`: The number of bytes used for padding
    /// - `f64`: The efficiency percentage (data bytes / total size * 100)
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::structs::StructLayout;
    /// use coffee::backend::memory::types::Layout;
    /// 
    /// // Create a layout for a struct with padding
    /// let layouts = vec![
    ///     Layout::new(1, 1),   // 1 byte field
    ///     Layout::new(8, 8),   // 8 byte field requiring 7 bytes of padding after first field
    /// ];
    /// 
    /// let layout = StructLayout::calculate(&layouts);
    /// let (data_bytes, padding_bytes, efficiency) = layout.efficiency_metrics();
    /// 
    /// println!("Data: {} bytes, Padding: {} bytes, Efficiency: {:.1}%", 
    ///          data_bytes, padding_bytes, efficiency);
    /// ```
    pub fn efficiency_metrics(&self) -> (u64, u64, f64) {
        let data_bytes: u64 = self.fields.iter().map(|f| f.layout.size.bytes).sum();
        let padding_bytes = self.size - data_bytes;
        let efficiency = if self.size > 0 {
            (data_bytes as f64 / self.size as f64) * 100.0
        } else {
            100.0
        };
        (data_bytes, padding_bytes, efficiency)
    }
}
