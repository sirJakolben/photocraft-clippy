//! The tool tree: root → tools → groups → sub tools, each sub tool with its settings.
//!
//! Every node keeps its whole `Node` row and every sub tool its whole `Variant` rows, so columns this
//! crate does not interpret are written back unchanged. Typed accessors read the columns PhotoCraft
//! maps.

use std::collections::{HashMap, HashSet};

use crate::blobs::{MaterialRefs, ShowParams};
use crate::effector::Effector;
use crate::value::{Row, Value};
use crate::{Error, Limits, Result, check_limit, known, malformed};

/// Column names of the `Node` and `Manager` tables used for the tree.
pub(crate) mod col {
    pub const UUID: &str = "NodeUuid";
    pub const NAME: &str = "NodeName";
    pub const FIRST_CHILD: &str = "NodeFirstChildUuid";
    pub const NEXT: &str = "NodeNextUuid";
    pub const SELECTED: &str = "NodeSelectedUuid";
    pub const VARIANT: &str = "NodeVariantID";
    pub const INIT_VARIANT: &str = "NodeInitVariantID";
    pub const INPUT_OP: &str = "NodeInputOp";
    pub const OUTPUT_OP: &str = "NodeOutputOp";
    pub const RANGE_OP: &str = "NodeRangeOp";
    pub const ICON: &str = "NodeIcon";
    pub const ICON_COLOR: &str = "NodeIconColor";
    pub const CUSTOM_ICON: &str = "NodeCustomIcon";
    pub const SHORTCUT: &str = "NodeShortCutKey";
    pub const HIDDEN: &str = "NodeHidden";
    pub const LOCK: &str = "NodeLock";
    pub const DEFAULT_IDENTIFIER: &str = "NodeDefaultIdentifier";
    pub const ROOT: &str = "RootUuid";
    pub const CURRENT: &str = "CurrentNodeUuid";
    pub const VARIANT_ID: &str = "VariantID";
}

/// The "no node" UUID Clip Studio Paint writes: one zero byte.
pub(crate) const NULL_UUID: [u8; 1] = [0];

/// Whether a node UUID column means "none": `NULL`, an empty blob or only zero bytes.
pub fn is_null_uuid(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Blob(b)) => b.iter().all(|&x| x == 0),
        Some(_) => false,
    }
}

/// One `sqlite_master` entry of the source file, in file order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaObject {
    /// `table` or `index` (other kinds are not kept).
    pub kind: String,
    /// Object name.
    pub name: String,
    /// The table it belongs to.
    pub table: String,
    /// The `CREATE` statement, verbatim.
    pub sql: Option<String>,
}

/// Everything about the database that is not the tree: schema, page size, the `Manager` row and
/// the `sqlite_sequence` counters. The `.sut` writer uses it as its template.
#[derive(Clone, Debug, PartialEq)]
pub struct DbInfo {
    /// SQLite page size of the source.
    pub page_size: u32,
    /// Text encoding of the source (`UTF-8` for `.sut`, `UTF-16le` for `.todb`).
    pub encoding: String,
    /// Tables and indices in file order.
    pub objects: Vec<SchemaObject>,
    /// Columns (name, declared type) per table, in table order.
    pub columns: Vec<(String, Vec<(String, String)>)>,
    /// The single `Manager` row without `_PW_ID` (root UUID, version, global pressure graph …).
    pub manager: Row,
    /// `sqlite_sequence` counters as stored.
    pub sequence: Vec<(String, i64)>,
}

impl DbInfo {
    /// The columns of `table`, if the schema has it.
    pub fn table_columns(&self, table: &str) -> Option<&[(String, String)]> {
        self.columns.iter().find(|(t, _)| t.eq_ignore_ascii_case(table)).map(|(_, c)| c.as_slice())
    }

    /// Whether the schema has `table`.
    pub fn has_table(&self, table: &str) -> bool {
        self.table_columns(table).is_some()
    }

    /// `Manager.Version` (138 for Clip Studio Paint 1.5).
    pub fn version(&self) -> Option<i64> {
        self.manager.int("Version")
    }
}

/// One `Variant` row: all settings of a sub tool.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Variant {
    /// The row without `_PW_ID`, `VariantID` included.
    pub row: Row,
}

impl Variant {
    /// `VariantID`.
    pub fn id(&self) -> Option<i64> {
        self.row.int(col::VARIANT_ID)
    }

    /// Value of a column.
    pub fn get(&self, column: &str) -> Option<&Value> {
        self.row.get(column)
    }

    /// Integer column.
    pub fn int(&self, column: &str) -> Option<i64> {
        self.row.int(column)
    }

    /// Numeric column as `f64`.
    pub fn real(&self, column: &str) -> Option<f64> {
        self.row.real(column)
    }

    /// Integer column as a flag (`!= 0`).
    pub fn flag(&self, column: &str) -> Option<bool> {
        self.row.int(column).map(|v| v != 0)
    }

    /// Text column.
    pub fn text(&self, column: &str) -> Option<&str> {
        self.row.text(column)
    }

    /// Blob column.
    pub fn blob(&self, column: &str) -> Option<&[u8]> {
        self.row.blob(column)
    }

    /// Sets a column (kept in place if it exists).
    pub fn set(&mut self, column: &str, value: Value) {
        self.row.set(column, value);
    }

    /// Decodes an `…Effector` column; `None` when the column is missing or `NULL`.
    pub fn effector(&self, column: &str) -> Option<Result<Effector>> {
        self.blob(column).map(Effector::decode)
    }

    /// Writes an effector into a column.
    pub fn set_effector(&mut self, column: &str, effector: &Effector) {
        self.set(column, Value::Blob(effector.encode()));
    }

    /// Decodes `VariantShowParam`.
    pub fn show_params(&self) -> Option<Result<ShowParams>> {
        self.blob(known::SHOW_PARAM).map(ShowParams::decode)
    }

    /// Decodes a material list column (`BrushPatternImageArray`, `TextureImage`, `Dual…`).
    pub fn material_refs(&self, column: &str) -> Option<Result<MaterialRefs>> {
        self.blob(column).map(MaterialRefs::decode)
    }

    /// Every material this variant references, from all material list columns; undecodable lists
    /// are skipped (their bytes stay in the row).
    pub fn all_material_refs(&self) -> Vec<crate::MaterialRef> {
        known::MATERIAL_COLUMNS
            .iter()
            .filter_map(|c| self.material_refs(c).and_then(Result::ok))
            .flat_map(|r| r.entries)
            .collect()
    }
}

/// One `MaterialFile` row of a `.sut`: a brush tip or texture embedded in the file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    /// The row without `_PW_ID` (`InstallFolder`, `OriginalPath`, `OldMaterial`, `FileData`,
    /// `CatalogPath`, `MaterialUuid`).
    pub row: Row,
}

impl Material {
    /// `CatalogPath`, the path variant blobs use to refer to this material.
    pub fn catalog_path(&self) -> Option<&str> {
        self.row.text("CatalogPath")
    }

    /// `OriginalPath`, the catalog path of the tip file itself.
    pub fn original_path(&self) -> Option<&str> {
        self.row.text("OriginalPath")
    }

    /// The material UUID from the catalog path.
    pub fn uuid(&self) -> Option<&str> {
        self.catalog_path().and_then(|p| p.rsplit(':').next()).filter(|u| !u.is_empty())
    }

    /// `FileData`: a tar archive (`catalog.zip`, `info.zip`, `data/*.layer`, thumbnail, layer XML).
    pub fn file_data(&self) -> Option<&[u8]> {
        self.row.blob("FileData")
    }
}

/// A sub tool: one `Node` row with its current and initial settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubTool {
    /// The `Node` row without `_PW_ID`.
    pub node: Row,
    /// Current settings (`NodeVariantID`).
    pub variant: Option<Variant>,
    /// Settings "Reset to initial settings" restores (`NodeInitVariantID`).
    pub init_variant: Option<Variant>,
    /// Materials embedded in a `.sut`; empty for sub tools of a `.todb`, whose materials are
    /// installed separately and referenced by the variant's material lists.
    pub materials: Vec<Material>,
}

macro_rules! node_accessors {
    () => {
        /// `NodeName`.
        pub fn name(&self) -> &str {
            self.node.text(col::NAME).unwrap_or_default()
        }

        /// Renames the node.
        pub fn set_name(&mut self, name: &str) {
            self.node.set(col::NAME, Value::Text(name.to_owned()));
        }

        /// `NodeUuid` (16 bytes in observed files).
        pub fn uuid(&self) -> &[u8] {
            self.node.blob(col::UUID).unwrap_or_default()
        }

        /// `NodeIcon`: the built-in icon id.
        pub fn icon(&self) -> Option<i64> {
            self.node.int(col::ICON)
        }

        /// `NodeIconColor`.
        pub fn icon_color(&self) -> Option<i64> {
            self.node.int(col::ICON_COLOR)
        }

        /// `NodeCustomIcon`: an own icon image, if set.
        pub fn custom_icon(&self) -> Option<&[u8]> {
            self.node.blob(col::CUSTOM_ICON)
        }

        /// `NodeShortCutKey` (0 = none; Clip Studio Paint's own key code).
        pub fn shortcut(&self) -> Option<i64> {
            self.node.int(col::SHORTCUT).filter(|&k| k != 0)
        }

        /// `NodeHidden`.
        pub fn hidden(&self) -> bool {
            self.node.int(col::HIDDEN).is_some_and(|v| v != 0)
        }
    };
}

impl SubTool {
    node_accessors!();

    /// `NodeInputOp`: how the tool takes input (the first half of its behaviour).
    pub fn input_op(&self) -> Option<i64> {
        self.node.int(col::INPUT_OP)
    }

    /// `NodeOutputOp`: what the tool does with it.
    pub fn output_op(&self) -> Option<i64> {
        self.node.int(col::OUTPUT_OP)
    }

    /// `NodeRangeOp`.
    pub fn range_op(&self) -> Option<i64> {
        self.node.int(col::RANGE_OP)
    }

    /// `NodeLock`: settings locked against changes.
    pub fn lock(&self) -> bool {
        self.node.int(col::LOCK).is_some_and(|v| v != 0)
    }

    /// `NodeDefaultIdentifier`: id of Clip Studio Paint's built-in sub tool this one is (0 = own).
    pub fn default_identifier(&self) -> Option<i64> {
        self.node.int(col::DEFAULT_IDENTIFIER)
    }
}

/// A sub tool group (the tabs of the Sub Tool palette).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    /// The `Node` row without `_PW_ID`.
    pub node: Row,
    /// Sub tools in palette order.
    pub subtools: Vec<SubTool>,
}

impl Group {
    node_accessors!();
}

/// A tool (an entry of the Tool palette).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tool {
    /// The `Node` row without `_PW_ID`.
    pub node: Row,
    /// Groups in palette order.
    pub groups: Vec<Group>,
}

impl Tool {
    node_accessors!();
}

/// The tool tree of a `.todb`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolTree {
    /// The root node row.
    pub root: Row,
    /// Tools in palette order.
    pub tools: Vec<Tool>,
}

impl ToolTree {
    /// Every sub tool with its tool and group.
    pub fn subtools(&self) -> impl Iterator<Item = (&Tool, &Group, &SubTool)> {
        self.tools.iter().flat_map(|t| t.groups.iter().flat_map(move |g| g.subtools.iter().map(move |s| (t, g, s))))
    }

    /// Number of groups.
    pub fn group_count(&self) -> usize {
        self.tools.iter().map(|t| t.groups.len()).sum()
    }
}

/// A Clip Studio Paint tool database (`EditImageTool.todb`).
#[derive(Clone, Debug, PartialEq)]
pub struct Todb {
    /// Schema, `Manager` row and counters.
    pub info: DbInfo,
    /// The tree.
    pub tree: ToolTree,
}

/// One exported sub tool (`.sut`).
#[derive(Clone, Debug, PartialEq)]
pub struct Sut {
    /// Schema, `Manager` row and counters; the template for writing.
    pub info: DbInfo,
    /// The sub tool.
    pub subtool: SubTool,
}

impl Sut {
    /// A `.sut` for `subtool` laid out like `template` (the [`DbInfo`] of a `.sut` or `.todb` read
    /// before): same tables and columns, page size and `Manager` values, with the root pointing at
    /// the sub tool. A `.todb` template has no `MaterialFile` table; the observed definition is
    /// added.
    pub fn from_subtool(subtool: SubTool, template: &DbInfo) -> Self {
        let mut info = template.clone();
        info.manager.set(col::ROOT, Value::Blob(subtool.uuid().to_vec()));
        info.manager.set(col::CURRENT, Value::Blob(NULL_UUID.to_vec()));
        info.sequence.clear();
        if !info.has_table(known::MATERIAL_TABLE) {
            info.objects.push(SchemaObject {
                kind: "table".into(),
                name: known::MATERIAL_TABLE.into(),
                table: known::MATERIAL_TABLE.into(),
                sql: Some(known::MATERIAL_FILE_SQL.into()),
            });
            info.columns.push((
                known::MATERIAL_TABLE.into(),
                known::MATERIAL_FILE_COLUMNS.iter().map(|(n, t)| ((*n).to_owned(), (*t).to_owned())).collect(),
            ));
        }
        Self { info, subtool }
    }
}

/// The rows of a tool database before they become a tree.
pub(crate) struct RawDb {
    pub info: DbInfo,
    pub nodes: Vec<Row>,
    pub variants: Vec<Row>,
    pub materials: Vec<Row>,
}

impl RawDb {
    /// Builds the tool tree of a `.todb`: root → tools → groups → sub tools, linked by
    /// `NodeFirstChildUuid` and `NodeNextUuid`. A node reached twice (a cycle or a shared node), a
    /// missing node or a fourth level is an error.
    pub(crate) fn into_todb(self, limits: &Limits) -> Result<Todb> {
        check_limit("node count", self.nodes.len() as u64, limits.max_nodes)?;
        let index = node_index(&self.nodes)?;
        let variants = variant_index(&self.variants);
        let root_uuid = self.info.manager.get(col::ROOT);
        if is_null_uuid(root_uuid) {
            return malformed("node tree", "Manager has no RootUuid");
        }
        let root = lookup(&index, &self.nodes, root_uuid)?.ok_or_else(|| tree_error("RootUuid names no node"))?;
        let mut seen = HashSet::new();
        seen.insert(root.0);
        let mut tools = Vec::new();
        for tool in children(&index, &self.nodes, root.1, &mut seen)? {
            let mut groups = Vec::new();
            for group in children(&index, &self.nodes, tool, &mut seen)? {
                let mut subtools = Vec::new();
                for sub in children(&index, &self.nodes, group, &mut seen)? {
                    if !is_null_uuid(sub.get(col::FIRST_CHILD)) {
                        return Err(tree_error("a sub tool has children (more than three levels)"));
                    }
                    subtools.push(subtool(sub, &variants, Vec::new()));
                }
                groups.push(Group { node: group.clone(), subtools });
            }
            tools.push(Tool { node: tool.clone(), groups });
        }
        Ok(Todb { info: self.info, tree: ToolTree { root: root.1.clone(), tools } })
    }

    /// The single sub tool of a `.sut`: the node `RootUuid` names (or the only node).
    pub(crate) fn into_sut(self, limits: &Limits) -> Result<Sut> {
        check_limit("node count", self.nodes.len() as u64, limits.max_nodes)?;
        let index = node_index(&self.nodes)?;
        let node = match lookup(&index, &self.nodes, self.info.manager.get(col::ROOT))? {
            Some((_, node)) => node,
            None if self.nodes.len() == 1 => self.nodes.first().ok_or_else(|| tree_error("no node"))?,
            None => return Err(tree_error("RootUuid names no node")),
        };
        let variants = variant_index(&self.variants);
        let materials = self.materials.into_iter().map(|row| Material { row }).collect();
        let subtool = subtool(node, &variants, materials);
        Ok(Sut { info: self.info, subtool })
    }
}

fn tree_error(reason: &str) -> Error {
    Error::Malformed { what: "node tree", reason: reason.to_owned() }
}

fn node_index(nodes: &[Row]) -> Result<HashMap<&[u8], usize>> {
    let mut index = HashMap::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let Some(uuid) = node.blob(col::UUID).filter(|u| u.iter().any(|&b| b != 0)) else { continue };
        if index.insert(uuid, i).is_some() {
            return Err(tree_error("two nodes share one NodeUuid"));
        }
    }
    Ok(index)
}

fn variant_index(variants: &[Row]) -> HashMap<i64, &Row> {
    let mut index = HashMap::with_capacity(variants.len());
    for v in variants {
        if let Some(id) = v.int(col::VARIANT_ID) {
            index.entry(id).or_insert(v);
        }
    }
    index
}

fn lookup<'a>(index: &HashMap<&[u8], usize>, nodes: &'a [Row], uuid: Option<&Value>) -> Result<Option<(usize, &'a Row)>> {
    if is_null_uuid(uuid) {
        return Ok(None);
    }
    let Some(Value::Blob(bytes)) = uuid else { return Err(tree_error("a node link is not a blob")) };
    Ok(index.get(bytes.as_slice()).and_then(|&i| nodes.get(i).map(|n| (i, n))))
}

/// The children of `parent`: its first child, then that child's `NodeNextUuid` chain. `seen` holds
/// every node already placed, so a cycle ends with an error instead of looping.
fn children<'a>(
    index: &HashMap<&[u8], usize>,
    nodes: &'a [Row],
    parent: &Row,
    seen: &mut HashSet<usize>,
) -> Result<Vec<&'a Row>> {
    let mut out = Vec::new();
    let mut link = parent.get(col::FIRST_CHILD);
    while !is_null_uuid(link) {
        let (i, node) = lookup(index, nodes, link)?.ok_or_else(|| tree_error("a link names a missing node"))?;
        if !seen.insert(i) {
            return Err(tree_error("a node is reached twice (cycle)"));
        }
        out.push(node);
        link = node.get(col::NEXT);
    }
    Ok(out)
}

fn subtool(node: &Row, variants: &HashMap<i64, &Row>, materials: Vec<Material>) -> SubTool {
    let variant_of = |column: &str| {
        node.int(column).filter(|&id| id != 0).and_then(|id| variants.get(&id)).map(|row| Variant { row: (*row).clone() })
    };
    SubTool { node: node.clone(), variant: variant_of(col::VARIANT), init_variant: variant_of(col::INIT_VARIANT), materials }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(uuid: u8, name: &str, child: u8, next: u8) -> Row {
        let link = |u: u8| Value::Blob(if u == 0 { vec![0] } else { vec![u; 16] });
        let mut row = Row::new();
        row.set(col::UUID, link(uuid));
        row.set(col::NAME, Value::Text(name.into()));
        row.set(col::FIRST_CHILD, link(child));
        row.set(col::NEXT, link(next));
        row
    }

    fn raw(nodes: Vec<Row>) -> RawDb {
        let mut manager = Row::new();
        manager.set(col::ROOT, Value::Blob(vec![1; 16]));
        let info = DbInfo {
            page_size: 1024,
            encoding: "UTF-8".into(),
            objects: Vec::new(),
            columns: Vec::new(),
            manager,
            sequence: Vec::new(),
        };
        RawDb { info, nodes, variants: Vec::new(), materials: Vec::new() }
    }

    #[test]
    fn builds_three_levels() {
        let nodes = vec![node(1, "", 2, 0), node(2, "Pen", 3, 0), node(3, "Glatt", 4, 0), node(4, "G-Pen", 0, 5), node(5, "Fineliner", 0, 0)];
        let todb = raw(nodes).into_todb(&Limits::default()).unwrap();
        let names: Vec<_> = todb.tree.subtools().map(|(t, g, s)| format!("{}/{}/{}", t.name(), g.name(), s.name())).collect();
        assert_eq!(names, ["Pen/Glatt/G-Pen", "Pen/Glatt/Fineliner"]);
    }

    #[test]
    fn cycles_missing_nodes_and_depth_are_errors() {
        let cycle = vec![node(1, "", 2, 0), node(2, "a", 0, 3), node(3, "b", 0, 2)];
        assert!(raw(cycle).into_todb(&Limits::default()).is_err());
        let self_child = vec![node(1, "", 1, 0)];
        assert!(raw(self_child).into_todb(&Limits::default()).is_err());
        let missing = vec![node(1, "", 9, 0)];
        assert!(raw(missing).into_todb(&Limits::default()).is_err());
        let deep = vec![node(1, "", 2, 0), node(2, "t", 3, 0), node(3, "g", 4, 0), node(4, "s", 5, 0), node(5, "x", 0, 0)];
        assert!(raw(deep).into_todb(&Limits::default()).is_err());
        let dup = vec![node(1, "", 0, 0), node(1, "again", 0, 0)];
        assert!(raw(dup).into_todb(&Limits::default()).is_err());
        let no_root = vec![node(2, "", 0, 0)];
        assert!(raw(no_root).into_todb(&Limits::default()).is_err());
    }

    #[test]
    fn node_count_is_capped() {
        let nodes = vec![node(1, "", 0, 0), node(2, "a", 0, 0)];
        let limits = Limits { max_nodes: 1, ..Limits::default() };
        assert!(matches!(raw(nodes).into_todb(&limits), Err(Error::Limit { .. })));
    }
}
