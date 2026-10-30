//! Where the tools are shown: tabs of one workspace that do not cover each other ("panels",
//! egui_tiles), or floating windows like in Emu8086 ("classic windows").
//!
//! In both layouts the open flags (`App::windows`, `library.open`, `reference.open`) say which
//! tools are shown. In the panels layout a tool opened by its flag gets a tab in its group (or
//! a new group where the default layout has it), and closing the tab clears the flag. Dialogs
//! (options, about, unsaved changes, ...), the flags and the ASCII table float in both layouts,
//! as in Emu8086.

use egui::Ui;
use egui_tiles::{Container, Tile, TileId, Tiles, Tree};
use serde::{Deserialize, Serialize};

use crate::app::App;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// Tabs and panels that do not overlap.
    Panels,
    /// Floating windows, like in Emu8086 (the default: as few differences from it as possible).
    #[default]
    Classic,
}

/// A tool that can be a tab of the workspace or a floating window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    Editor,
    Emulator,
    Memory,
    Stack,
    Vars,
    Watch,
    Alu,
    Screen,
    Devices,
    Drive,
    Source,
    Listing,
    Symbols,
    Library,
    Reference,
    Breakpoints,
    CallStack,
    Console,
    Log,
}

/// Groups of tools: each group is one set of tabs in the default layout.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    /// Editor and the texts next to it (left column, top).
    Text,
    /// Breakpoints, call stack (left column, bottom).
    Debug,
    /// Registers and disassembly (middle column, top).
    Emulator,
    /// Memory, watch, ALU (middle column, bottom).
    Inspect,
    /// Screen (right column, top).
    Screen,
    /// Under the screen: what the emulator's buttons open (variables, stack, debug log), the log,
    /// devices and the drive (right column, bottom).
    Aside,
}

impl Group {
    /// Column of the default layout (0 = left).
    fn column(self) -> usize {
        match self {
            Group::Text | Group::Debug => 0,
            Group::Emulator | Group::Inspect => 1,
            Group::Screen | Group::Aside => 2,
        }
    }

    /// The group above this one in its column, or `None` for the top one.
    fn below(self) -> Option<Group> {
        match self {
            Group::Debug => Some(Group::Text),
            Group::Inspect => Some(Group::Emulator),
            Group::Aside => Some(Group::Screen),
            _ => None,
        }
    }

    /// Share of the column's height when the column is split.
    fn height(self) -> f32 {
        match self {
            Group::Text => 0.64,
            Group::Debug => 0.36,
            Group::Emulator => 0.72,
            Group::Inspect => 0.28,
            Group::Screen | Group::Aside => 0.5,
        }
    }
}

/// Share of the width of each default column.
const COLUMN_WIDTHS: [f32; 3] = [0.26, 0.42, 0.32];

impl Tool {
    /// All tools in the order the default layout places them.
    pub const ALL: [Tool; 19] = [
        Tool::Editor,
        Tool::Source,
        Tool::Listing,
        Tool::Symbols,
        Tool::Library,
        Tool::Reference,
        Tool::Emulator,
        Tool::Memory,
        Tool::Watch,
        Tool::Alu,
        Tool::Screen,
        Tool::Vars,
        Tool::Stack,
        Tool::Console,
        Tool::Log,
        Tool::Devices,
        Tool::Drive,
        Tool::Breakpoints,
        Tool::CallStack,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tool::Editor => "editor",
            Tool::Emulator => "emulator",
            Tool::Memory => "memory",
            Tool::Stack => "stack",
            Tool::Vars => "variables",
            Tool::Watch => "watch",
            Tool::Alu => "ALU",
            Tool::Screen => "screen",
            Tool::Devices => "virtual devices",
            Tool::Drive => "drive C:",
            Tool::Source => "source",
            Tool::Listing => "listing",
            Tool::Symbols => "symbol table",
            Tool::Library => "emu8086.inc",
            Tool::Reference => "reference",
            Tool::Breakpoints => "breakpoints",
            Tool::CallStack => "call stack",
            Tool::Console => "debug log",
            Tool::Log => "log",
        }
    }

    fn group(self) -> Group {
        match self {
            Tool::Editor | Tool::Source | Tool::Listing | Tool::Symbols | Tool::Library | Tool::Reference => {
                Group::Text
            }
            Tool::Breakpoints | Tool::CallStack => Group::Debug,
            Tool::Emulator => Group::Emulator,
            Tool::Memory | Tool::Watch | Tool::Alu => Group::Inspect,
            Tool::Screen => Group::Screen,
            Tool::Vars | Tool::Stack | Tool::Console | Tool::Log | Tool::Devices | Tool::Drive => Group::Aside,
        }
    }

    /// The body scrolls by itself (otherwise its tab gets a scroll area).
    fn scrolls(self) -> bool {
        matches!(
            self,
            Tool::Editor
                | Tool::Screen
                | Tool::Source
                | Tool::Memory
                | Tool::Stack
                | Tool::Vars
                | Tool::Symbols
                | Tool::Listing
                | Tool::Log
                | Tool::Console
                | Tool::Drive
                | Tool::Library
                | Tool::Reference
        )
    }

    fn flag(self, app: &mut App) -> Option<&mut bool> {
        let w = &mut app.windows;
        Some(match self {
            Tool::Editor => return None,
            Tool::Emulator => &mut w.emulator,
            Tool::Memory => &mut w.memory,
            Tool::Stack => &mut w.stack,
            Tool::Vars => &mut w.vars,
            Tool::Watch => &mut w.watch,
            Tool::Alu => &mut w.alu,
            Tool::Screen => &mut w.screen,
            Tool::Devices => &mut w.devices,
            Tool::Drive => &mut w.drive,
            Tool::Source => &mut w.source,
            Tool::Listing => &mut w.listing,
            Tool::Symbols => &mut w.symbols,
            Tool::Library => &mut app.library.open,
            Tool::Reference => &mut app.reference.open,
            Tool::Breakpoints => &mut w.breakpoints,
            Tool::CallStack => &mut w.call_stack,
            Tool::Console => &mut w.console,
            Tool::Log => &mut w.log,
        })
    }

    /// Whether the tool is shown (the editor always is).
    pub fn is_open(self, app: &mut App) -> bool {
        self.flag(app).is_none_or(|f| *f)
    }

    fn close(self, app: &mut App) {
        if let Some(f) = self.flag(app) {
            *f = false;
        }
    }
}

/// The body of a tool. Tools of the interface crate (emulator, source) come from `extra`, which
/// returns false for the others.
pub fn tool_ui(app: &mut App, ui: &mut Ui, tool: Tool, extra: &mut dyn FnMut(&mut App, &mut Ui, Tool) -> bool) {
    if extra(app, ui, tool) {
        return;
    }
    match tool {
        Tool::Editor => crate::editor::editor(app, ui),
        Tool::Screen => crate::screen::screen_ui(app, ui),
        Tool::Memory => crate::debug::memory_ui(app, ui),
        Tool::Stack => crate::debug::stack_ui(app, ui),
        Tool::Vars => crate::debug::vars_ui(app, ui),
        Tool::Symbols => crate::debug::symbols_ui(app, ui),
        Tool::Listing => crate::debug::listing_ui(app, ui),
        Tool::Log => crate::debug::log_ui(app, ui),
        Tool::Watch => crate::debugger::watch_ui(app, ui),
        Tool::Alu => crate::debugger::alu_ui(app, ui),
        Tool::Breakpoints => crate::debugger::breakpoints_ui(app, ui),
        Tool::CallStack => crate::debugger::call_stack_ui(app, ui),
        Tool::Devices => crate::devices_ui::devices_ui(app, ui),
        Tool::Drive => crate::drive_ui::drive_ui(app, ui),
        Tool::Console => crate::console::console_ui(app, ui),
        Tool::Library => crate::library_ui::library_ui(app, ui),
        Tool::Reference => crate::reference_ui::reference_ui(&mut app.reference, ui),
        Tool::Emulator | Tool::Source => {
            ui.weak("not available here");
        }
    }
}

/// The workspace of the panels layout: tabs of the open tools.
pub fn panels_ui(app: &mut App, ui: &mut Ui, extra: &mut dyn FnMut(&mut App, &mut Ui, Tool) -> bool) {
    let mut tree = app.panels.take().unwrap_or_else(|| new_tree(app));
    sync(&mut tree, app);
    for (raise, tool) in [(&mut app.reference.raise, Tool::Reference), (&mut app.library.raise, Tool::Library)] {
        if std::mem::take(raise)
            && let Some(tile) = tree.tiles.find_pane(&tool)
        {
            activate(&mut tree, tile);
        }
    }
    let mut behavior = Behavior { app, extra, closed: Vec::new() };
    tree.ui(&mut behavior, ui);
    let Behavior { app, closed, .. } = behavior;
    for t in closed {
        t.close(app);
    }
    app.panels = Some(tree);
}

/// Puts the open tools where the default layout has them (panels), or the windows where they
/// first appear (classic windows).
pub fn reset_layout(app: &mut App, ctx: &egui::Context) {
    if app.panels() {
        app.panels = None;
    } else {
        ctx.memory_mut(|m| m.reset_areas());
    }
}

/// The choice of the layout and its reset (options window).
pub fn layout_options(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.settings.layout, Layout::Classic, "classic windows (Emu8086)")
            .on_hover_text("Every tool is a floating window, like in the original Emu8086");
        ui.radio_value(&mut app.settings.layout, Layout::Panels, "panels")
            .on_hover_text("Tools are tabs that can be dragged and resized; nothing covers anything");
        if ui.button("reset").on_hover_text("Put the windows back where they start").clicked() {
            reset_layout(app, ui.ctx());
        }
    });
}

/// The default layout with the open tools.
fn new_tree(app: &mut App) -> Tree<Tool> {
    let mut tiles = Tiles::default();
    let editor = tiles.insert_pane(Tool::Editor);
    let root = tiles.insert_tab_tile(vec![editor]);
    let mut tree = Tree::new(egui::Id::unique("panels"), root, tiles);
    sync(&mut tree, app);
    // The editor stays the visible tab of its group.
    activate(&mut tree, editor);
    tree
}

/// Removes the tabs of closed tools and adds tabs for newly opened ones.
fn sync(tree: &mut Tree<Tool>, app: &mut App) {
    let panes: Vec<(TileId, Tool)> = tree
        .tiles
        .iter()
        .filter_map(|(&id, t)| match t {
            Tile::Pane(p) => Some((id, *p)),
            _ => None,
        })
        .collect();
    for (id, tool) in panes {
        if !tool.is_open(app) {
            tree.remove_recursively(id);
        }
    }
    for tool in Tool::ALL {
        if tool.is_open(app) && tree.tiles.find_pane(&tool).is_none() {
            insert(tree, tool);
        }
    }
}

fn activate(tree: &mut Tree<Tool>, tile: TileId) {
    if let Some(parent) = tree.tiles.parent_of(tile)
        && let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent)
    {
        tabs.set_active(tile);
    }
}

/// Group of the tools under `tile` (the first pane found).
fn group_of(tiles: &Tiles<Tool>, tile: TileId) -> Option<Group> {
    match tiles.get(tile)? {
        Tile::Pane(p) => Some(p.group()),
        Tile::Container(c) => c.children().find_map(|&c| group_of(tiles, c)),
    }
}

/// Tabs container that holds a tool of `group`.
fn tabs_of(tree: &Tree<Tool>, group: Group, except: TileId) -> Option<TileId> {
    tree.tiles.iter().find_map(|(&id, t)| match t {
        Tile::Pane(p) if id != except && p.group() == group => tree
            .tiles
            .parent_of(id)
            .filter(|&parent| matches!(tree.tiles.get(parent), Some(Tile::Container(Container::Tabs(_))))),
        _ => None,
    })
}

/// Adds a tab for `tool`: next to the tools of its group, or as a new group where the default
/// layout has it. A new tab becomes the visible one of its group, except the source (opened on
/// every emulate), which does not hide the editor.
fn insert(tree: &mut Tree<Tool>, tool: Tool) {
    let pane = tree.tiles.insert_pane(tool);
    let group = tool.group();
    if let Some(tabs) = tabs_of(tree, group, pane) {
        if let Some(Tile::Container(Container::Tabs(t))) = tree.tiles.get_mut(tabs) {
            t.add_child(pane);
            let editor_shown = t.active.is_some_and(|a| tree.tiles.get_pane(&a) == Some(&Tool::Editor));
            if !(editor_shown && tool == Tool::Source)
                && let Some(Tile::Container(Container::Tabs(t))) = tree.tiles.get_mut(tabs)
            {
                t.set_active(pane);
            }
        }
        return;
    }
    let tabs = tree.tiles.insert_tab_tile(vec![pane]);
    let Some(root) = tree.root else {
        tree.root = Some(tabs);
        return;
    };
    // Below or above the other group of its column.
    let partner = match group.below() {
        Some(above) => Some((above, true)),
        None => [Group::Debug, Group::Inspect, Group::Aside]
            .into_iter()
            .find(|g| g.below() == Some(group))
            .map(|below| (below, false)),
    };
    if let Some((other, below)) = partner
        && let Some(other_tabs) = tabs_of(tree, other, pane)
    {
        let parent = tree.tiles.parent_of(other_tabs);
        let children = if below { vec![other_tabs, tabs] } else { vec![tabs, other_tabs] };
        let column = tree.tiles.insert_vertical_tile(children);
        if let Some(Tile::Container(Container::Linear(l))) = tree.tiles.get_mut(column) {
            l.shares.set_share(tabs, group.height());
            l.shares.set_share(other_tabs, other.height());
        }
        match parent {
            Some(p) => {
                if let Some(Tile::Container(c)) = tree.tiles.get_mut(p) {
                    let _ = c.replace_child(other_tabs, column);
                }
            }
            None => tree.root = Some(column),
        }
        return;
    }
    // A new column, ordered as in the default layout.
    let horizontal = match tree.tiles.get(root) {
        Some(Tile::Container(Container::Linear(l))) if l.dir == egui_tiles::LinearDir::Horizontal => root,
        _ => {
            let h = tree.tiles.insert_horizontal_tile(vec![root]);
            let width = group_of(&tree.tiles, root).map_or(COLUMN_WIDTHS[0], |g| COLUMN_WIDTHS[g.column()]);
            if let Some(Tile::Container(Container::Linear(l))) = tree.tiles.get_mut(h) {
                l.shares.set_share(root, width);
            }
            tree.root = Some(h);
            h
        }
    };
    let columns: Vec<Option<usize>> = match tree.tiles.get(horizontal) {
        Some(Tile::Container(c)) => c.children().map(|&c| group_of(&tree.tiles, c).map(Group::column)).collect(),
        _ => Vec::new(),
    };
    let at = columns.iter().take_while(|c| c.is_some_and(|c| c <= group.column())).count();
    if let Some(Tile::Container(Container::Linear(l))) = tree.tiles.get_mut(horizontal) {
        l.children.insert(at, tabs);
        l.shares.set_share(tabs, COLUMN_WIDTHS[group.column()]);
    }
}

struct Behavior<'a> {
    app: &'a mut App,
    extra: &'a mut dyn FnMut(&mut App, &mut Ui, Tool) -> bool,
    closed: Vec<Tool>,
}

impl egui_tiles::Behavior<Tool> for Behavior<'_> {
    fn tab_title_for_pane(&mut self, tool: &Tool) -> egui::WidgetText {
        tool.title().into()
    }

    fn pane_ui(&mut self, ui: &mut Ui, _tile: TileId, tool: &mut Tool) -> egui_tiles::UiResponse {
        let tool = *tool;
        let fill = ui.visuals().panel_fill;
        egui::Frame::new().fill(fill).inner_margin(6.0).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.push_id(tool, |ui| {
                if tool.scrolls() {
                    tool_ui(self.app, ui, tool, self.extra);
                } else {
                    // The content gets the height of the panel (the emulator fits its list to it) and
                    // scrolls when it is taller.
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| tool_ui(self.app, ui, tool, self.extra));
                }
            });
        });
        egui_tiles::UiResponse::None
    }

    fn is_tab_closable(&self, tiles: &Tiles<Tool>, tile: TileId) -> bool {
        tiles.get_pane(&tile) != Some(&Tool::Editor)
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<Tool>, tile: TileId) -> bool {
        if let Some(&tool) = tiles.get_pane(&tile) {
            self.closed.push(tool);
        }
        true
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions { all_panes_must_have_tabs: true, ..Default::default() }
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        3.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(tree: &Tree<Tool>, tile: TileId) -> String {
        match tree.tiles.get(tile).unwrap() {
            Tile::Pane(p) => p.title().to_string(),
            Tile::Container(c) => {
                let kind = match c {
                    Container::Tabs(_) => "tabs",
                    Container::Linear(l) if l.dir == egui_tiles::LinearDir::Horizontal => "h",
                    Container::Linear(_) => "v",
                    Container::Grid(_) => "grid",
                };
                let children: Vec<String> = c.children().map(|&c| layout(tree, c)).collect();
                format!("{kind}[{}]", children.join(", "))
            }
        }
    }

    #[test]
    fn tools_go_to_their_groups() {
        let mut app = App::with_settings(crate::app::Settings::default());
        let mut tree = new_tree(&mut app);
        assert_eq!(layout(&tree, tree.root.unwrap()), "tabs[editor]");
        // emulate: emulator, screen, source.
        (app.windows.emulator, app.windows.screen, app.windows.source) = (true, true, true);
        sync(&mut tree, &mut app);
        assert_eq!(layout(&tree, tree.root.unwrap()), "h[tabs[editor, source], tabs[emulator], tabs[screen]]");
        // The editor stays visible.
        let Some(Tile::Container(Container::Tabs(t))) =
            tree.tiles.get(tree.tiles.parent_of(tree.tiles.find_pane(&Tool::Editor).unwrap()).unwrap())
        else {
            panic!()
        };
        assert_eq!(tree.tiles.get_pane(&t.active.unwrap()), Some(&Tool::Editor));
        (app.windows.memory, app.windows.stack, app.windows.devices, app.windows.breakpoints) =
            (true, true, true, true);
        sync(&mut tree, &mut app);
        assert_eq!(
            layout(&tree, tree.root.unwrap()),
            "h[v[tabs[editor, source], tabs[breakpoints]], v[tabs[emulator], tabs[memory]], \
             v[tabs[screen], tabs[stack, virtual devices]]]"
        );
        // Closed tools lose their tabs; an empty group goes away when the tree is simplified.
        app.windows.emulator = false;
        sync(&mut tree, &mut app);
        tree.simplify(&egui_tiles::SimplificationOptions { all_panes_must_have_tabs: true, ..Default::default() });
        assert!(tree.tiles.find_pane(&Tool::Emulator).is_none());
        // The emulator comes back above the memory.
        app.windows.emulator = true;
        sync(&mut tree, &mut app);
        let s = layout(&tree, tree.root.unwrap());
        assert!(s.contains("v[tabs[emulator], tabs[memory]]"), "{s}");
        // A tool opened when nothing of its column is open gets a column of its own.
        let mut app = App::with_settings(crate::app::Settings::default());
        app.windows.watch = true;
        let tree = new_tree(&mut app);
        assert_eq!(layout(&tree, tree.root.unwrap()), "h[tabs[editor], tabs[watch]]");
    }
}
