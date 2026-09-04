//! Layout Collection and Reporting
//! 
//! Collects memory layout information during compilation and generates comprehensive reports.
//! This module provides facilities for tracking and analyzing the memory layout of all
//! data structures in a compilation unit, including structs, classes, and other composite
//! types. The collected information is used to generate detailed reports about memory
//! usage, efficiency metrics, and potential optimization opportunities.

use super::structs::StructLayout;
use std::string::String;

/// Memory layout collector for compilation unit
/// 
/// This structure manages the collection of memory layout information for all
/// structs defined during compilation. It tracks struct layouts, generates
/// comprehensive reports, and provides analysis tools to help developers
/// optimize their data structures for better memory usage.
/// 
/// The collector maintains a registry of all structs with their names,
/// layouts, and field information, enabling detailed analysis of memory
/// efficiency across the entire compilation unit.
#[derive(Debug, Clone, Default)]
pub struct LayoutCollector {
    /// Vector of struct layout information collected during compilation
    pub structs: Vec<StructLayoutInfo>,
}

/// Information about a single struct's layout
/// 
/// This structure holds detailed information about a single struct,
/// including its name, memory layout, and field names. It serves as
/// a container for all the information needed to analyze and report
/// on the memory layout of a specific struct.
#[derive(Debug, Clone)]
pub struct StructLayoutInfo {
    /// Name of the struct
    pub name: String,
    /// Memory layout of the struct (fields, sizes, alignment, padding)
    pub layout: StructLayout,
    /// Names of the fields in the struct (in the order they were defined)
    pub field_names: Vec<String>,
}

impl LayoutCollector {
    /// Create a new layout collector
    /// 
    /// Initializes an empty layout collector that can be used to track
    /// struct layouts during compilation. The collector starts with
    /// no structs and can have them added using the `add_struct` method.
    /// 
    /// # Returns
    /// 
    /// A new LayoutCollector instance ready to collect struct layout information
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::collector::LayoutCollector;
    /// 
    /// let mut collector = LayoutCollector::new();
    /// // Now you can add structs to the collector
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a struct to the layout collector
    /// 
    /// This method adds information about a single struct to the collector,
    /// including its name, memory layout, and field names. This information
    /// is stored and used when generating reports.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the struct
    /// * `layout` - The memory layout of the struct (calculated by StructLayout::calculate)
    /// * `field_names` - Vector of field names in the order they were defined
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::collector::LayoutCollector;
    /// use coffee::backend::memory::structs::{StructLayout, StructLayoutInfo};
    /// use coffee::backend::memory::types::Layout;
    /// 
    /// let mut collector = LayoutCollector::new();
    /// 
    /// // Create a simple layout for demonstration
    /// let layouts = vec![Layout::new(4, 4), Layout::new(8, 8)];
    /// let layout = StructLayout::calculate(&layouts);
    /// 
    /// collector.add_struct(
    ///     "ExampleStruct".to_string(),
    ///     layout,
    ///     vec!["field1".to_string(), "field2".to_string()]
    /// );
    /// ```
    pub fn add_struct(&mut self, name: String, layout: StructLayout, field_names: Vec<String>) {
        self.structs.push(StructLayoutInfo {
            name,
            layout,
            field_names,
        });
    }

    /// Generate complete memory layout report
    /// 
    /// This method creates a comprehensive report of all collected struct
    /// layouts, including summary statistics, efficiency metrics, and
    /// detailed memory maps for each struct. The report is formatted as
    /// a human-readable string with visual elements to make it easy to
    /// understand and analyze.
    /// 
    /// The report includes:
    /// - Summary statistics for all structs
    /// - Detailed information for each struct
    /// - Memory maps showing field placement and padding
    /// - Efficiency metrics for each struct and overall
    /// 
    /// # Returns
    /// 
    /// A formatted string containing the complete memory layout analysis report
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::collector::LayoutCollector;
    /// 
    /// let collector = LayoutCollector::new();
    /// let report = collector.generate_report();
    /// println!("{}", report);
    /// ```
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        report.push_str("╔════════════════════════════════════════════════════════════════╗\n");
        report.push_str("║           Memory Layout Analysis Report                              ║\n");
        report.push_str("╚════════════════════════════════════════════════════════════════╝\n\n");

        if self.structs.is_empty() {
            report.push_str("No structs defined.\n");
            return report;
        }

        // Summary statistics
        let total_data: u64 = self.structs.iter()
            .map(|s| s.layout.efficiency_metrics().0)
            .sum();
        let total_padding: u64 = self.structs.iter()
            .map(|s| s.layout.efficiency_metrics().1)
            .sum();
        let total_size: u64 = self.structs.iter()
            .map(|s| s.layout.size)
            .sum();
        let overall_efficiency = if total_size > 0 {
            (total_data as f64 / total_size as f64) * 100.0
        } else {
            100.0
        };

        report.push_str("📊 Summary:\n");
        report.push_str(&format!("  Total structs: {}\n", self.structs.len()));
        report.push_str(&format!("  Total size: {} bytes\n", total_size));
        report.push_str(&format!("  Data: {} bytes ({:.1}%)\n", total_data, overall_efficiency));
        if total_padding > 0 {
            report.push_str(&format!("  Padding: {} bytes ({:.1}%)\n", total_padding, 100.0 - overall_efficiency));
        }
        report.push_str("\n");

        // Per-struct details
        report.push_str("📦 Struct Details:\n");
        report.push_str(&str::repeat("─", 70));
        report.push_str("\n");

        for (idx, struct_info) in self.structs.iter().enumerate() {
            let (data, padding, efficiency) = struct_info.layout.efficiency_metrics();

            report.push_str(&format!("\n[{}] {}\n", idx + 1, struct_info.name));
            report.push_str(&format!("  Size: {} bytes\n", struct_info.layout.size));
            report.push_str(&format!("  Data: {} bytes\n", data));
            if padding > 0 {
                report.push_str(&format!("  Padding: {} bytes\n", padding));
            }
            report.push_str(&format!("  Efficiency: {:.1}%\n", efficiency));
            report.push_str(&format!("  Alignment: {} bytes\n", struct_info.layout.align));

            // Memory map visualization
            report.push_str("\n  Memory Map:\n");
            let mut offset = 0;
            for field in struct_info.layout.fields.iter() {
                let field_name = &struct_info.field_names[field.original_index];
                let gap = field.offset - offset;

                if gap > 0 {
                    report.push_str(&format!("    [0x{:04x}-0x{:04x}] <{} bytes padding>\n",
                        offset, offset + gap, gap));
                    offset += gap;
                }

                let field_end = offset + field.layout.size.bytes;
                report.push_str(&format!("    [0x{:04x}-0x{:04x}] {}: {} bytes, align {}\n",
                    offset, field_end, field_name, field.layout.size.bytes, field.layout.align.value));
                    offset = field_end;
            }

            // Tail padding
            if offset < struct_info.layout.size {
                let tail_padding = struct_info.layout.size - offset;
                report.push_str(&format!("    [0x{:04x}-0x{:04x}] <{} bytes tail padding>\n",
                    offset, struct_info.layout.size, tail_padding));
            }

            report.push_str("\n");
            report.push_str(&str::repeat("─", 70));
            report.push_str("\n");
        }

        report.push_str("\n");
        report
    }
}
