use parquet::basic::Type as PhysicalType;
use parquet::file::reader::FileReader;
use parquet::file::serialized_reader::SerializedFileReader;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct SchemaSummary {
    pub path: PathBuf,
    pub row_count: i64,
    pub row_group_count: usize,
    pub column_count: usize,
    pub created_by: Option<String>,
    pub time_column: Option<ColumnInfo>,
    pub channels: Vec<ColumnInfo>,
    pub columns: Vec<ColumnInfo>,
}

impl SchemaSummary {
    pub fn to_report(&self) -> String {
        let mut report = String::new();

        let _ = writeln!(report, "file: {}", self.path.display());
        let _ = writeln!(report, "rows: {}", self.row_count);
        let _ = writeln!(report, "row groups: {}", self.row_group_count);
        let _ = writeln!(report, "columns: {}", self.column_count);
        if let Some(created_by) = &self.created_by {
            let _ = writeln!(report, "created by: {created_by}");
        }
        match &self.time_column {
            Some(time_column) => {
                let _ = writeln!(report, "time column: {}", time_column.display_name());
            }
            None => {
                let _ = writeln!(report, "time column: <not found>");
            }
        }

        let _ = writeln!(report);
        let _ = writeln!(report, "channels:");
        for channel in &self.channels {
            let _ = writeln!(
                report,
                "  #{:02} {} ({})",
                channel.index,
                channel.display_name(),
                channel.physical_type
            );
        }

        let _ = writeln!(report);
        let _ = writeln!(report, "columns:");
        for column in &self.columns {
            let logical = column.logical_type.as_deref().unwrap_or("-");
            let numeric = if column.is_numeric { "yes" } else { "no" };
            let _ = writeln!(
                report,
                "  #{:02} {:<7} {:<8} {:<32} logical={} converted={} numeric={}",
                column.index,
                column.role.as_str(),
                column.physical_type,
                column.display_name(),
                logical,
                column.converted_type,
                numeric
            );
        }

        report
    }
}

#[derive(Clone, Debug)]
pub struct ColumnInfo {
    pub index: usize,
    pub name: String,
    pub path: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub converted_type: String,
    pub is_numeric: bool,
    pub role: ColumnRole,
}

impl ColumnInfo {
    pub fn display_name(&self) -> &str {
        if self.path == self.name {
            &self.name
        } else {
            &self.path
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnRole {
    Time,
    Channel,
    Ignored,
}

impl ColumnRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Channel => "channel",
            Self::Ignored => "ignored",
        }
    }
}

pub fn read_schema_summary(path: impl AsRef<Path>) -> Result<SchemaSummary, String> {
    let path = path.as_ref();
    let reader: SerializedFileReader<std::fs::File> =
        SerializedFileReader::try_from(path).map_err(|error| error.to_string())?;
    let metadata = reader.metadata();
    let file_metadata = metadata.file_metadata();
    let schema = file_metadata.schema_descr();

    let mut columns = Vec::with_capacity(schema.num_columns());
    let mut time_column = None;
    let mut channels = Vec::new();

    for (index, descriptor) in schema.columns().iter().enumerate() {
        let physical_type = descriptor.physical_type();
        let name = descriptor.name().to_owned();
        let path = descriptor.path().string();
        let is_time = is_time_column(&name, &path);
        let is_numeric = is_numeric_physical_type(physical_type);
        let role = if is_time {
            ColumnRole::Time
        } else if is_numeric {
            ColumnRole::Channel
        } else {
            ColumnRole::Ignored
        };

        let info = ColumnInfo {
            index,
            name,
            path,
            physical_type: format!("{physical_type:?}"),
            logical_type: descriptor
                .logical_type_ref()
                .map(|logical_type| format!("{logical_type:?}")),
            converted_type: format!("{:?}", descriptor.converted_type()),
            is_numeric,
            role,
        };

        if role == ColumnRole::Time && time_column.is_none() {
            time_column = Some(info.clone());
        } else if role == ColumnRole::Channel {
            channels.push(info.clone());
        }

        columns.push(info);
    }

    Ok(SchemaSummary {
        path: path.to_path_buf(),
        row_count: file_metadata.num_rows(),
        row_group_count: metadata.num_row_groups(),
        column_count: schema.num_columns(),
        created_by: file_metadata.created_by().map(str::to_owned),
        time_column,
        channels,
        columns,
    })
}

fn is_time_column(name: &str, path: &str) -> bool {
    name.eq_ignore_ascii_case("time") || path.eq_ignore_ascii_case("time")
}

fn is_numeric_physical_type(physical_type: PhysicalType) -> bool {
    matches!(
        physical_type,
        PhysicalType::INT32 | PhysicalType::INT64 | PhysicalType::FLOAT | PhysicalType::DOUBLE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_flat_phase1_dataset_if_available() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../proto_3_1b/data/test_100k.parquet");

        if !path.exists() {
            return;
        }

        let summary = read_schema_summary(path).expect("read schema");

        assert_eq!(summary.row_count, 100_000);
        assert_eq!(
            summary
                .time_column
                .as_ref()
                .map(|column| column.display_name()),
            Some("time")
        );
        assert_eq!(summary.channels.len(), 4);
        assert_eq!(summary.channels[0].display_name(), "sine_50Hz");
    }
}
