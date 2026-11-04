//! The panels layout: the tools with a group are tabs of one workspace that do not cover each
//! other (egui_tiles). A tool opened by its flag in [`crate::tools::Windows`] gets a tab in its group
//! (or a new group where the default layout has it), and closing the tab clears the flag. The
//! classic layout (floating windows, like in Emu8086) is drawn by [`crate::tools::windows`].

use egui::Ui;
use egui_tiles::{Container, Tile, TileId, Tiles, Tree};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::tools::{Host, Tool, tool_ui};

/// How the tools are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// Tabs and panels that do not overlap.
    Panels,
    /// Floating windows, like in Emu8086 (the default: as few differences from it as possible).
    #[default]
    Classic,
}

/// Groups of tools: each group is one set of tabs in the default layout.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
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

/// Share of the width of each default column: the 8086's screen takes a wide one, MIPS's Run I/O
/// console a narrow one, leaving room to the emulator's code list with its source column.
fn column_widths(app: &App) -> [f32; 3] {
    match app.arch() {
        rustemu_session::Arch::I8086 => [0.26, 0.42, 0.32],
        rustemu_session::Arch::Mips32 => [0.25, 0.53, 0.22],
    }
}

/// The workspace of the panels layout: tabs of the open tools.
pub fn panels_ui(app: &mut App, ui: &mut Ui, host: &mut dyn Host) {
    // A tool asked to be shown (an article of the reference, ...) opens and becomes the visible tab.
    let raised: Vec<Tool> = Tool::ALL.into_iter().filter(|t| t.raise(app).is_some_and(std::mem::take)).collect();
    for &t in &raised {
        app.windows[t] = true;
    }
    let mut tree = app.panels.take().unwrap_or_else(|| new_tree(app));
    sync(&mut tree, app);
    for tool in raised {
        if let Some(tile) = tree.tiles.find_pane(&tool) {
            activate(&mut tree, tile);
        }
    }
    let mut behavior = Behavior { app, host, closed: Vec::new() };
    tree.ui(&mut behavior, ui);
    let Behavior { app, closed, .. } = behavior;
    for t in closed {
        app.windows[t] = false;
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
        // (A tool of a saved tree may have become a floating one.)
        if !app.windows.is_open(tool) || tool.group().is_none() {
            tree.remove_recursively(id);
        }
    }
    for tool in Tool::ALL.into_iter().filter(|t| t.group().is_some()) {
        if app.windows.is_open(tool) && tree.tiles.find_pane(&tool).is_none() {
            insert(tree, tool, column_widths(app));
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
        Tile::Pane(p) => p.group(),
        Tile::Container(c) => c.children().find_map(|&c| group_of(tiles, c)),
    }
}

/// Tabs container that holds a tool of `group`.
fn tabs_of(tree: &Tree<Tool>, group: Group, except: TileId) -> Option<TileId> {
    tree.tiles.iter().find_map(|(&id, t)| match t {
        Tile::Pane(p) if id != except && p.group() == Some(group) => tree
            .tiles
            .parent_of(id)
            .filter(|&parent| matches!(tree.tiles.get(parent), Some(Tile::Container(Container::Tabs(_))))),
        _ => None,
    })
}

/// Adds a tab for `tool`: next to the tools of its group, or as a new group where the default
/// layout has it. A new tab becomes the visible one of its group, except the source (opened on
/// every emulate), which does not hide the editor.
fn insert(tree: &mut Tree<Tool>, tool: Tool, widths: [f32; 3]) {
    let Some(group) = tool.group() else { return };
    let pane = tree.tiles.insert_pane(tool);
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
            let width = group_of(&tree.tiles, root).map_or(widths[0], |g| widths[g.column()]);
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
        l.shares.set_share(tabs, widths[group.column()]);
    }
}

struct Behavior<'a> {
    app: &'a mut App,
    host: &'a mut dyn Host,
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
                    tool_ui(self.app, ui, tool, self.host);
                } else {
                    // The content gets the height of the panel (the emulator fits its list to it) and
                    // scrolls when it is taller.
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| tool_ui(self.app, ui, tool, self.host));
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
        (app.windows[Tool::Emulator], app.windows[Tool::Screen], app.windows[Tool::Source]) = (true, true, true);
        sync(&mut tree, &mut app);
        assert_eq!(layout(&tree, tree.root.unwrap()), "h[tabs[editor, source], tabs[emulator], tabs[screen]]");
        // The editor stays visible.
        let Some(Tile::Container(Container::Tabs(t))) =
            tree.tiles.get(tree.tiles.parent_of(tree.tiles.find_pane(&Tool::Editor).unwrap()).unwrap())
        else {
            panic!()
        };
        assert_eq!(tree.tiles.get_pane(&t.active.unwrap()), Some(&Tool::Editor));
        (
            app.windows[Tool::Watch],
            app.windows[Tool::Stack],
            app.windows[Tool::Devices],
            app.windows[Tool::Breakpoints],
        ) = (true, true, true, true);
        sync(&mut tree, &mut app);
        assert_eq!(
            layout(&tree, tree.root.unwrap()),
            "h[v[tabs[editor, source], tabs[breakpoints]], v[tabs[emulator], tabs[watch]], \
             v[tabs[screen], tabs[stack, virtual devices]]]"
        );
        // Closed tools lose their tabs; an empty group goes away when the tree is simplified.
        app.windows[Tool::Emulator] = false;
        sync(&mut tree, &mut app);
        tree.simplify(&egui_tiles::SimplificationOptions { all_panes_must_have_tabs: true, ..Default::default() });
        assert!(tree.tiles.find_pane(&Tool::Emulator).is_none());
        // The emulator comes back above the watch.
        app.windows[Tool::Emulator] = true;
        sync(&mut tree, &mut app);
        let s = layout(&tree, tree.root.unwrap());
        assert!(s.contains("v[tabs[emulator], tabs[watch]]"), "{s}");
        // A tool opened when nothing of its column is open gets a column of its own.
        let mut app = App::with_settings(crate::app::Settings::default());
        app.windows[Tool::Watch] = true;
        let tree = new_tree(&mut app);
        assert_eq!(layout(&tree, tree.root.unwrap()), "h[tabs[editor], tabs[watch]]");
    }
}
