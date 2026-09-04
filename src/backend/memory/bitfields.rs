//! Bit Field Support
//! 
//! Provides bit field layout and storage optimization for the Coffee compiler.
//! This module implements efficient packing of multiple small values into
//! a single storage unit, allowing for significant memory savings when
//! dealing with many small fields or flags. The bit field system provides
//! automatic optimization of storage allocation and detailed analysis of
//! bit field efficiency.

/// Bit field specification for layout calculation
/// 
/// This structure represents a single bit field definition, containing
/// the name and width in bits. BitFieldSpec is used as input to the
/// layout calculation algorithms to determine optimal packing strategies.
/// 
/// The width is specified in bits and must be between 1 and 64, as
/// larger bit fields would require more complex implementation.
#[derive(Debug, Clone)]
pub struct BitFieldSpec {
    /// Name of the bit field (used for identification and debugging)
    pub name: String,
    /// Width of the field in bits (1-64)
    pub width: u8,
}

/// Bit field specification with offset
/// 
/// This structure represents a bit field that has been assigned a specific
/// location within a storage unit. It includes the field name, its bit
/// offset from the start of the storage unit, and its width in bits.
/// 
/// BitField instances are created by the layout calculation algorithms
/// and represent the final placement of bit fields within storage units.
#[derive(Debug, Clone)]
pub struct BitField {
    /// Name of the bit field
    pub name: String,
    /// Bit offset from start of storage unit
    pub offset: u8,
    /// Bit width of the field (1-64)
    pub width: u8,
}

/// Storage unit types for bit fields
/// 
/// This enumeration defines the possible storage unit types for bit fields.
/// Different storage types allow for different maximum bit counts and
/// provide different performance characteristics. The selection of storage
/// type is automatically handled by the layout algorithms based on the
/// total width required by the bit fields.
#[derive(Debug, Clone, Copy)]
pub enum StorageType {
    /// 8-bit unsigned integer storage (max 8 bits)
    U8,
    /// 16-bit unsigned integer storage (max 16 bits)
    U16,
    /// 32-bit unsigned integer storage (max 32 bits)
    U32,
    /// 64-bit unsigned integer storage (max 64 bits)
    U64,
}

impl StorageType {
    /// Get the number of bits in the storage type
    /// 
    /// # Returns
    /// 
    /// The number of bits available in this storage type (8, 16, 32, or 64)
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::bitfields::StorageType;
    /// 
    /// assert_eq!(StorageType::U32.bits(), 32);
    /// assert_eq!(StorageType::U64.bits(), 64);
    /// ```
    pub fn bits(&self) -> u8 {
        match self {
            StorageType::U8 => 8,
            StorageType::U16 => 16,
            StorageType::U32 => 32,
            StorageType::U64 => 64,
        }
    }

    /// Get the number of bytes in the storage type
    /// 
    /// # Returns
    /// 
    /// The number of bytes required to store this storage type (1, 2, 4, or 8)
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::bitfields::StorageType;
    /// 
    /// assert_eq!(StorageType::U16.bytes(), 2);
    /// assert_eq!(StorageType::U64.bytes(), 8);
    /// ```
    pub fn bytes(&self) -> u8 {
        self.bits() / 8
    }
}

/// Bit field layout information
/// 
/// This structure represents the complete layout of multiple bit fields
/// within a single storage unit. It includes the selected storage type
/// and the specific placement of each bit field within that unit.
/// 
/// BitFieldLayout is the result of the layout calculation algorithms
/// and represents an optimized packing of multiple fields into the
/// smallest possible storage unit.
#[derive(Debug, Clone)]
pub struct BitFieldLayout {
    /// The storage type chosen for the bit field collection
    pub storage_type: StorageType,
    /// Vector of bit fields with their specific offsets and widths
    pub fields: Vec<BitField>,
}

impl BitFieldLayout {
    /// Calculate optimal bit field storage
    /// 
    /// This function calculates an optimal layout for a collection of bit fields,
    /// placing them sequentially in the smallest possible storage unit. The
    /// algorithm simply packs the fields in the order they are provided, as
    /// no reordering is needed for bit fields (unlike struct fields where
    /// alignment matters).
    /// 
    /// The storage type is automatically selected based on the total number
    /// of bits required by all fields combined.
    /// 
    /// # Arguments
    /// 
    /// * `specs` - Slice of BitFieldSpec instances describing the fields to pack
    /// 
    /// # Returns
    /// 
    /// A BitFieldLayout instance with optimized field placement
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::bitfields::{BitFieldLayout, BitFieldSpec};
    /// 
    /// let specs = vec![
    ///     BitFieldSpec { name: "flag1".to_string(), width: 1 },
    ///     BitFieldSpec { name: "flag2".to_string(), width: 1 },
    ///     BitFieldSpec { name: "value".to_string(), width: 6 },
    /// ];
    /// 
    /// let layout = BitFieldLayout::calculate(&specs);
    /// assert_eq!(layout.storage_type.bytes(), 1); // U8 is sufficient for 8 bits
    /// assert_eq!(layout.fields.len(), 3); // All 3 fields are included
    /// ```
    pub fn calculate(specs: &[BitFieldSpec]) -> Self {
        let total_bits: u8 = specs.iter().map(|f| f.width).sum();

        // Choose smallest storage unit that fits
        let storage_type = if total_bits <= 8 {
            StorageType::U8
        } else if total_bits <= 16 {
            StorageType::U16
        } else if total_bits <= 32 {
            StorageType::U32
        } else {
            StorageType::U64
        };

        let mut offset = 0u8;
        let fields: Vec<BitField> = specs.iter().map(|spec| {
            let field = BitField {
                name: spec.name.clone(),
                offset,
                width: spec.width,
            };
            offset += spec.width;
            field
        }).collect();

        BitFieldLayout {
            storage_type,
            fields,
        }
    }

    /// Get total storage size in bytes
    /// 
    /// This function returns the total size in bytes of the storage unit
    /// used for the bit fields. This is determined by the selected storage type.
    /// 
    /// # Returns
    /// 
    /// The storage size in bytes (1, 2, 4, or 8 depending on storage type)
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::bitfields::{BitFieldLayout, BitFieldSpec, StorageType};
    /// 
    /// let specs = vec![BitFieldSpec { name: "field".to_string(), width: 5 }];
    /// let layout = BitFieldLayout::calculate(&specs);
    /// 
    /// assert_eq!(layout.storage_size(), StorageType::U8.bytes());
    /// ```
    pub fn storage_size(&self) -> u8 {
        self.storage_type.bytes()
    }

    /// Calculate storage efficiency (bits used / total bits)
    /// 
    /// This function computes the efficiency of the bit field layout by
    /// comparing the number of bits actually used by fields to the total
    /// number of bits available in the storage unit. This metric helps
    /// determine how effectively the storage unit is being utilized.
    /// 
    /// # Returns
    /// 
    /// The efficiency percentage as a floating-point number between 0.0 and 100.0
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::bitfields::{BitFieldLayout, BitFieldSpec};
    /// 
    /// // 5 bits in an 8-bit field = 62.5% efficiency
    /// let specs = vec![BitFieldSpec { name: "field".to_string(), width: 5 }];
    /// let layout = BitFieldLayout::calculate(&specs);
    /// assert_eq!(layout.efficiency(), 62.5);
    /// 
    /// // 8 bits in an 8-bit field = 100% efficiency
    /// let specs = vec![
    ///     BitFieldSpec { name: "field1".to_string(), width: 3 },
    ///     BitFieldSpec { name: "field2".to_string(), width: 5 }
    /// ];
    /// let layout = BitFieldLayout::calculate(&specs);
    /// assert_eq!(layout.efficiency(), 100.0);
    /// ```
    pub fn efficiency(&self) -> f64 {
        let used_bits: u8 = self.fields.iter().map(|f| f.width).sum();
        let total_bits = self.storage_type.bits();
        (used_bits as f64 / total_bits as f64) * 100.0
    }
}
