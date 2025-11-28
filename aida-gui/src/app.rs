use aida_core::{
    determine_requirements_path, Cardinality, Comment, CustomFieldDefinition, CustomFieldType,
    FieldChange, IdFormat, NumberingStrategy, RelationshipDefinition, RelationshipType,
    Requirement, RequirementPriority, RequirementStatus, RequirementType, RequirementsStore,
    Storage, UrlLink,
};
use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use uuid::Uuid;

// =============================================================================
// TEXT EDIT CONTEXT MENU IMPLEMENTATION
// =============================================================================
//
// BACKGROUND:
// egui's TextEdit widget clears the text selection when a right-click occurs,
// before any user code can intercept it. This is a known limitation.
// See: https://github.com/emilk/egui/issues/5852
//
// WORKAROUND:
// We continuously capture the selection state while the TextEdit has focus.
// This way, when the user right-clicks, we already have the selection stored
// from the previous frame, before the right-click event cleared it.
//
// LIMITATIONS:
// 1. The visual selection highlight disappears when the context menu opens
//    (we show the selected text in the menu as a workaround)
// 2. Selection is not preserved in the TextEdit after menu closes
//
// FUTURE IMPROVEMENTS (check these egui issues/features):
// - Issue #5852: Add support for X11/Wayland PRIMARY clipboard
//   https://github.com/emilk/egui/issues/5852
// - Issue #7273: TextSelectionChanged events for all selectable text
//   https://github.com/emilk/egui/issues/7273
// - If egui adds a way to prevent right-click from clearing selection,
//   or provides a "selection changed" callback, this code can be simplified
// - If egui adds native context menu support for TextEdit with Copy/Paste,
//   this entire workaround may become unnecessary
//
// TO UPDATE WHEN EGUI IMPROVES:
// 1. If egui preserves selection on right-click: remove continuous capture,
//    just read selection directly in context_menu callback
// 2. If egui adds TextSelectionChanged event: use that instead of polling
// 3. If egui adds native context menu: remove this entirely
// =============================================================================

/// Helper to get selection range from CCursorRange (handles primary/secondary ordering)
fn get_sorted_range(range: &egui::text::CCursorRange) -> (usize, usize) {
    let start = range.primary.index.min(range.secondary.index);
    let end = range.primary.index.max(range.secondary.index);
    (start, end)
}

/// Capture current text selection from a TextEdit widget.
///
/// This reads the TextEdit's internal state to get the current cursor/selection range.
/// Returns None if there's no selection (cursor is just a point, not a range).
fn capture_text_selection(ctx: &egui::Context, text: &str, id: egui::Id) -> Option<TextSelection> {
    let state = egui::TextEdit::load_state(ctx, id)?;
    let range = state.cursor.char_range()?;
    let (start, end) = get_sorted_range(&range);
    if start == end {
        return None; // No selection (just cursor position)
    }
    let selected: String = text.chars().skip(start).take(end - start).collect();
    Some(TextSelection {
        text: selected,
        start,
        end,
        widget_id: Some(id),
    })
}

/// Show context menu for TextEdit with Cut/Copy/Paste/Select All.
///
/// # Workaround for egui selection clearing
///
/// egui's TextEdit clears the selection when right-click occurs. To work around this,
/// we continuously capture the selection while the TextEdit has focus. The selection
/// is stored in `stored_selection` and used when the context menu opens.
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `response` - The Response from the TextEdit widget
/// * `text` - Mutable reference to the text buffer
/// * `id` - The widget ID (use `response.id`)
/// * `stored_selection` - Mutable reference to stored selection state (persists across frames)
///
/// # Linux Primary Selection
/// On Linux, Copy and Cut also write to the X11/Wayland PRIMARY selection,
/// enabling middle-click paste in other applications.
fn show_text_context_menu(
    ui: &mut egui::Ui,
    response: &egui::Response,
    text: &mut String,
    id: egui::Id,
    stored_selection: &mut Option<TextSelection>,
) {
    // WORKAROUND: Continuously capture selection while TextEdit has focus.
    // This ensures we have the selection stored BEFORE right-click clears it.
    // See module-level documentation for details on this workaround.
    if response.has_focus() {
        if let Some(selection) = capture_text_selection(ui.ctx(), text, id) {
            // Only update if this is for the same widget and selection changed
            let should_update = stored_selection
                .as_ref()
                .map(|s| s.widget_id != Some(id) || s.text != selection.text)
                .unwrap_or(true);
            if should_update {
                *stored_selection = Some(selection);
            }
        }
    }

    response.context_menu(|ui| {
        // Use stored selection (captured continuously while focused, before right-click cleared it)
        let selection = stored_selection.clone().filter(|s| s.widget_id == Some(id));
        let has_selection = selection
            .as_ref()
            .map(|s| !s.text.is_empty())
            .unwrap_or(false);

        // WORKAROUND: Show selected text in menu since visual selection is cleared
        // This lets users see what they're about to copy/cut
        if let Some(ref sel) = selection {
            let display_text = if sel.text.len() > 50 {
                format!("\"{}...\"", &sel.text.chars().take(47).collect::<String>())
            } else {
                format!("\"{}\"", &sel.text)
            };
            // Replace newlines with visible indicator
            let display_text = display_text.replace('\n', "↵");
            ui.label(egui::RichText::new(display_text).italics().weak());
            ui.separator();
        }

        // Cut
        if ui
            .add_enabled(has_selection, egui::Button::new("✂ Cut"))
            .clicked()
        {
            if let Some(ref sel) = selection {
                // Copy to clipboard (both regular and primary on Linux)
                ui.ctx().copy_text(sel.text.clone());
                copy_to_primary_selection(&sel.text);

                // Remove the selected text
                let before: String = text.chars().take(sel.start).collect();
                let after: String = text.chars().skip(sel.end).collect();
                *text = before + &after;
                *stored_selection = None;
            }
            ui.close_menu();
        }

        // Copy
        if ui
            .add_enabled(has_selection, egui::Button::new("📋 Copy"))
            .clicked()
        {
            if let Some(ref sel) = selection {
                // Copy to clipboard (both regular and primary on Linux)
                ui.ctx().copy_text(sel.text.clone());
                copy_to_primary_selection(&sel.text);
            }
            ui.close_menu();
        }

        // Paste
        let can_paste = get_clipboard_text().is_some();

        if ui
            .add_enabled(can_paste, egui::Button::new("📥 Paste"))
            .clicked()
        {
            if let Some(paste_text) = get_clipboard_text() {
                if let Some(ref sel) = selection {
                    // Replace selection with pasted text
                    let before: String = text.chars().take(sel.start).collect();
                    let after: String = text.chars().skip(sel.end).collect();
                    *text = before + &paste_text + &after;
                } else {
                    // No selection - get cursor position from state
                    if let Some(state) = egui::TextEdit::load_state(ui.ctx(), id) {
                        if let Some(range) = state.cursor.char_range() {
                            let pos = range.primary.index;
                            let before: String = text.chars().take(pos).collect();
                            let after: String = text.chars().skip(pos).collect();
                            *text = before + &paste_text + &after;
                        } else {
                            text.push_str(&paste_text);
                        }
                    } else {
                        text.push_str(&paste_text);
                    }
                }
                *stored_selection = None;
            }
            ui.close_menu();
        }

        ui.separator();

        // Select All
        if ui.button("Select All").clicked() {
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                let len = text.chars().count();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(len),
                    )));
                state.store(ui.ctx(), id);
            }
            ui.close_menu();
        }
    });
}

/// Copy text to the X11/Wayland primary selection (middle-click paste buffer).
///
/// On Linux/X11, there are two clipboard mechanisms:
/// 1. CLIPBOARD - Used by Ctrl+C/Ctrl+V (handled by egui's ctx.copy_text())
/// 2. PRIMARY - Filled by selecting text, pasted with middle-click
///
/// egui only handles CLIPBOARD, so we use arboard to also copy to PRIMARY.
/// This enables middle-click paste in other applications after copying from our app.
///
/// This is a workaround until egui adds native PRIMARY selection support.
/// See: https://github.com/emilk/egui/issues/5852
#[cfg(target_os = "linux")]
fn copy_to_primary_selection(text: &str) {
    use arboard::SetExtLinux;
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        // Set to primary selection for middle-click paste
        let _ = clipboard
            .set()
            .clipboard(arboard::LinuxClipboardKind::Primary)
            .text(text.to_string());
    }
}

/// No-op on non-Linux platforms (PRIMARY selection is X11/Wayland specific)
#[cfg(not(target_os = "linux"))]
fn copy_to_primary_selection(_text: &str) {}

/// Get text from the system clipboard (CLIPBOARD, not PRIMARY)
fn get_clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok())
}

/// Default base font size in points
const DEFAULT_FONT_SIZE: f32 = 14.0;
/// Minimum font size
const MIN_FONT_SIZE: f32 = 8.0;
/// Maximum font size
const MAX_FONT_SIZE: f32 = 32.0;
/// Font size step for zoom in/out
const FONT_SIZE_STEP: f32 = 1.0;

/// Serializable color wrapper for themes
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThemeColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ThemeColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn with_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.r, self.g, self.b, self.a)
    }

    pub fn from_egui(color: egui::Color32) -> Self {
        Self {
            r: color.r(),
            g: color.g(),
            b: color.b(),
            a: color.a(),
        }
    }
}

impl Default for ThemeColor {
    fn default() -> Self {
        Self::new(128, 128, 128)
    }
}

/// Helper to show a color picker widget
fn color_picker_widget(ui: &mut egui::Ui, color: &mut ThemeColor) {
    let mut color32 = color.to_egui();

    egui::color_picker::color_edit_button_srgba(
        ui,
        &mut color32,
        egui::color_picker::Alpha::Opaque,
    );

    *color = ThemeColor::from_egui(color32);
}

/// Custom theme with full visual customization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTheme {
    /// Name of this custom theme
    pub name: String,
    /// Base theme to extend from
    pub base: BaseTheme,

    // === Background Colors ===
    /// Main window background
    pub window_fill: ThemeColor,
    /// Panel background color
    pub panel_fill: ThemeColor,
    /// Extreme background (e.g., text edit background)
    pub extreme_bg: ThemeColor,
    /// Faint background (subtle separators)
    pub faint_bg: ThemeColor,

    // === Text Colors ===
    /// Primary text color (None = use widget defaults)
    pub text_color: Option<ThemeColor>,
    /// Hyperlink color
    pub hyperlink_color: ThemeColor,
    /// Warning text color
    pub warn_fg: ThemeColor,
    /// Error text color
    pub error_fg: ThemeColor,

    // === Widget Colors - Noninteractive ===
    /// Non-interactive widget background
    pub widget_bg: ThemeColor,
    /// Non-interactive widget text/stroke color
    pub widget_fg: ThemeColor,

    // === Widget Colors - Interactive States ===
    /// Inactive/default widget background
    pub widget_inactive_bg: ThemeColor,
    /// Hovered widget background
    pub widget_hovered_bg: ThemeColor,
    /// Active/pressed widget background
    pub widget_active_bg: ThemeColor,
    /// Open (expanded) widget background
    pub widget_open_bg: ThemeColor,

    // === Selection ===
    /// Selection background
    pub selection_bg: ThemeColor,
    /// Selection text color
    pub selection_fg: ThemeColor,

    // === Strokes & Borders ===
    /// Widget border width
    pub widget_stroke_width: f32,
    /// Widget border color (inactive)
    pub widget_stroke_color: ThemeColor,
    /// Widget border color (hovered)
    pub widget_hovered_stroke_color: ThemeColor,
    /// Widget border color (active)
    pub widget_active_stroke_color: ThemeColor,

    // === Rounding ===
    /// Widget corner rounding
    pub widget_rounding: f32,
    /// Window corner rounding
    pub window_rounding: f32,

    // === Shadows ===
    /// Window shadow enabled
    pub window_shadow: bool,
    /// Popup shadow enabled
    pub popup_shadow: bool,

    // === Spacing ===
    /// Item spacing (horizontal, vertical)
    pub item_spacing: (f32, f32),
    /// Button padding (horizontal, vertical)
    pub button_padding: (f32, f32),
    /// Window padding
    pub window_padding: (f32, f32),

    // === Miscellaneous ===
    /// Use dark mode icons/decorations
    pub dark_mode: bool,
    /// Scroll bar width
    pub scroll_bar_width: f32,
    /// Indent amount for nested items
    pub indent: f32,
}

impl Default for CustomTheme {
    fn default() -> Self {
        Self::from_base(BaseTheme::Dark, "Custom Dark".to_string())
    }
}

impl CustomTheme {
    /// Create a custom theme based on a built-in base theme
    pub fn from_base(base: BaseTheme, name: String) -> Self {
        match base {
            BaseTheme::Dark => Self::dark_defaults(name),
            BaseTheme::Light => Self::light_defaults(name),
        }
    }

    fn dark_defaults(name: String) -> Self {
        Self {
            name,
            base: BaseTheme::Dark,
            // Background
            window_fill: ThemeColor::new(27, 27, 27),
            panel_fill: ThemeColor::new(27, 27, 27),
            extreme_bg: ThemeColor::new(10, 10, 10),
            faint_bg: ThemeColor::new(5, 5, 5),
            // Text
            text_color: None,
            hyperlink_color: ThemeColor::new(90, 170, 255),
            warn_fg: ThemeColor::new(255, 143, 0),
            error_fg: ThemeColor::new(255, 0, 0),
            // Widgets - noninteractive
            widget_bg: ThemeColor::new(27, 27, 27),
            widget_fg: ThemeColor::new(140, 140, 140),
            // Widgets - interactive
            widget_inactive_bg: ThemeColor::new(60, 60, 60),
            widget_hovered_bg: ThemeColor::new(70, 70, 70),
            widget_active_bg: ThemeColor::new(55, 55, 55),
            widget_open_bg: ThemeColor::new(27, 27, 27),
            // Selection
            selection_bg: ThemeColor::new(0, 92, 128),
            selection_fg: ThemeColor::new(255, 255, 255),
            // Strokes
            widget_stroke_width: 1.0,
            widget_stroke_color: ThemeColor::new(60, 60, 60),
            widget_hovered_stroke_color: ThemeColor::new(150, 150, 150),
            widget_active_stroke_color: ThemeColor::new(255, 255, 255),
            // Rounding
            widget_rounding: 2.0,
            window_rounding: 6.0,
            // Shadows
            window_shadow: true,
            popup_shadow: true,
            // Spacing
            item_spacing: (8.0, 3.0),
            button_padding: (4.0, 1.0),
            window_padding: (6.0, 6.0),
            // Misc
            dark_mode: true,
            scroll_bar_width: 8.0,
            indent: 18.0,
        }
    }

    fn light_defaults(name: String) -> Self {
        Self {
            name,
            base: BaseTheme::Light,
            // Background
            window_fill: ThemeColor::new(248, 248, 248),
            panel_fill: ThemeColor::new(248, 248, 248),
            extreme_bg: ThemeColor::new(255, 255, 255),
            faint_bg: ThemeColor::new(245, 245, 245),
            // Text
            text_color: None,
            hyperlink_color: ThemeColor::new(0, 102, 204),
            warn_fg: ThemeColor::new(255, 100, 0),
            error_fg: ThemeColor::new(220, 0, 0),
            // Widgets - noninteractive
            widget_bg: ThemeColor::new(248, 248, 248),
            widget_fg: ThemeColor::new(100, 100, 100),
            // Widgets - interactive
            widget_inactive_bg: ThemeColor::new(230, 230, 230),
            widget_hovered_bg: ThemeColor::new(210, 210, 210),
            widget_active_bg: ThemeColor::new(195, 195, 195),
            widget_open_bg: ThemeColor::new(235, 235, 235),
            // Selection
            selection_bg: ThemeColor::new(144, 209, 255),
            selection_fg: ThemeColor::new(0, 0, 0),
            // Strokes
            widget_stroke_width: 1.0,
            widget_stroke_color: ThemeColor::new(180, 180, 180),
            widget_hovered_stroke_color: ThemeColor::new(105, 105, 105),
            widget_active_stroke_color: ThemeColor::new(0, 0, 0),
            // Rounding
            widget_rounding: 2.0,
            window_rounding: 6.0,
            // Shadows
            window_shadow: false,
            popup_shadow: true,
            // Spacing
            item_spacing: (8.0, 3.0),
            button_padding: (4.0, 1.0),
            window_padding: (6.0, 6.0),
            // Misc
            dark_mode: false,
            scroll_bar_width: 8.0,
            indent: 18.0,
        }
    }

    /// Apply this custom theme to the egui context
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        // Background colors
        visuals.window_fill = self.window_fill.to_egui();
        visuals.panel_fill = self.panel_fill.to_egui();
        visuals.extreme_bg_color = self.extreme_bg.to_egui();
        visuals.faint_bg_color = self.faint_bg.to_egui();

        // Text colors
        visuals.override_text_color = self.text_color.map(|c| c.to_egui());
        visuals.hyperlink_color = self.hyperlink_color.to_egui();
        visuals.warn_fg_color = self.warn_fg.to_egui();
        visuals.error_fg_color = self.error_fg.to_egui();

        // Widgets - noninteractive
        visuals.widgets.noninteractive.bg_fill = self.widget_bg.to_egui();
        visuals.widgets.noninteractive.fg_stroke =
            egui::Stroke::new(1.0, self.widget_fg.to_egui());

        // Widgets - interactive states
        visuals.widgets.inactive.bg_fill = self.widget_inactive_bg.to_egui();
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(self.widget_stroke_width, self.widget_stroke_color.to_egui());
        visuals.widgets.inactive.rounding = egui::Rounding::same(self.widget_rounding);

        visuals.widgets.hovered.bg_fill = self.widget_hovered_bg.to_egui();
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(
            self.widget_stroke_width,
            self.widget_hovered_stroke_color.to_egui(),
        );
        visuals.widgets.hovered.rounding = egui::Rounding::same(self.widget_rounding);

        visuals.widgets.active.bg_fill = self.widget_active_bg.to_egui();
        visuals.widgets.active.bg_stroke = egui::Stroke::new(
            self.widget_stroke_width,
            self.widget_active_stroke_color.to_egui(),
        );
        visuals.widgets.active.rounding = egui::Rounding::same(self.widget_rounding);

        visuals.widgets.open.bg_fill = self.widget_open_bg.to_egui();
        visuals.widgets.open.rounding = egui::Rounding::same(self.widget_rounding);

        // Selection
        visuals.selection.bg_fill = self.selection_bg.to_egui();
        visuals.selection.stroke = egui::Stroke::new(1.0, self.selection_fg.to_egui());

        // Window rounding
        visuals.window_rounding = egui::Rounding::same(self.window_rounding);

        // Shadows
        if !self.window_shadow {
            visuals.window_shadow = egui::Shadow::NONE;
        }
        if !self.popup_shadow {
            visuals.popup_shadow = egui::Shadow::NONE;
        }

        ctx.set_visuals(visuals);

        // Apply spacing settings
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(self.item_spacing.0, self.item_spacing.1);
        style.spacing.button_padding = egui::vec2(self.button_padding.0, self.button_padding.1);
        style.spacing.window_margin = egui::Margin::same(self.window_padding.0);
        style.spacing.scroll.bar_width = self.scroll_bar_width;
        style.spacing.indent = self.indent;
        ctx.set_style(style);
    }
}

/// Base theme to extend from
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BaseTheme {
    #[default]
    Dark,
    Light,
}

impl BaseTheme {
    fn label(&self) -> &'static str {
        match self {
            BaseTheme::Dark => "Dark",
            BaseTheme::Light => "Light",
        }
    }
}

/// Available color themes
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub enum Theme {
    /// Dark theme (default egui dark)
    #[default]
    Dark,
    /// Light theme
    Light,
    /// High contrast dark
    HighContrastDark,
    /// Solarized dark
    SolarizedDark,
    /// Nord theme
    Nord,
    /// Custom theme with user-defined colors
    Custom(Box<CustomTheme>),
}

impl Theme {
    fn label(&self) -> String {
        match self {
            Theme::Dark => "Dark".to_string(),
            Theme::Light => "Light".to_string(),
            Theme::HighContrastDark => "High Contrast Dark".to_string(),
            Theme::SolarizedDark => "Solarized Dark".to_string(),
            Theme::Nord => "Nord".to_string(),
            Theme::Custom(theme) => format!("Custom: {}", theme.name),
        }
    }

    fn next(&self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::HighContrastDark,
            Theme::HighContrastDark => Theme::SolarizedDark,
            Theme::SolarizedDark => Theme::Nord,
            Theme::Nord => Theme::Dark,
            Theme::Custom(_) => Theme::Dark, // Cycle back to Dark from Custom
        }
    }

    fn apply(&self, ctx: &egui::Context) {
        match self {
            Theme::Dark => {
                ctx.set_visuals(egui::Visuals::dark());
            }
            Theme::Light => {
                ctx.set_visuals(egui::Visuals::light());
            }
            Theme::HighContrastDark => {
                let mut visuals = egui::Visuals::dark();
                visuals.override_text_color = Some(egui::Color32::WHITE);
                visuals.widgets.noninteractive.bg_fill = egui::Color32::from_gray(20);
                visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(30);
                visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(50);
                visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                visuals.selection.bg_fill = egui::Color32::from_rgb(0, 100, 200);
                visuals.extreme_bg_color = egui::Color32::BLACK;
                ctx.set_visuals(visuals);
            }
            Theme::SolarizedDark => {
                let mut visuals = egui::Visuals::dark();
                // Solarized dark colors
                let base03 = egui::Color32::from_rgb(0, 43, 54);
                let base02 = egui::Color32::from_rgb(7, 54, 66);
                let base01 = egui::Color32::from_rgb(88, 110, 117);
                let base0 = egui::Color32::from_rgb(131, 148, 150);
                let base1 = egui::Color32::from_rgb(147, 161, 161);
                let cyan = egui::Color32::from_rgb(42, 161, 152);

                visuals.override_text_color = Some(base0);
                visuals.widgets.noninteractive.bg_fill = base03;
                visuals.widgets.noninteractive.fg_stroke.color = base1;
                visuals.widgets.inactive.bg_fill = base02;
                visuals.widgets.hovered.bg_fill = base01;
                visuals.widgets.active.bg_fill = cyan;
                visuals.selection.bg_fill = cyan;
                visuals.extreme_bg_color = base03;
                visuals.faint_bg_color = base02;
                ctx.set_visuals(visuals);
            }
            Theme::Nord => {
                let mut visuals = egui::Visuals::dark();
                // Nord colors
                let nord0 = egui::Color32::from_rgb(46, 52, 64); // Polar Night
                let nord1 = egui::Color32::from_rgb(59, 66, 82);
                let nord2 = egui::Color32::from_rgb(67, 76, 94);
                let nord3 = egui::Color32::from_rgb(76, 86, 106);
                let nord4 = egui::Color32::from_rgb(216, 222, 233); // Snow Storm
                let nord8 = egui::Color32::from_rgb(136, 192, 208); // Frost
                let nord10 = egui::Color32::from_rgb(94, 129, 172);

                visuals.override_text_color = Some(nord4);
                visuals.widgets.noninteractive.bg_fill = nord0;
                visuals.widgets.noninteractive.fg_stroke.color = nord4;
                visuals.widgets.inactive.bg_fill = nord1;
                visuals.widgets.hovered.bg_fill = nord2;
                visuals.widgets.active.bg_fill = nord3;
                visuals.selection.bg_fill = nord10;
                visuals.hyperlink_color = nord8;
                visuals.extreme_bg_color = nord0;
                visuals.faint_bg_color = nord1;
                ctx.set_visuals(visuals);
            }
            Theme::Custom(custom_theme) => {
                custom_theme.apply(ctx);
            }
        }
    }

    /// Check if this is a custom theme
    fn is_custom(&self) -> bool {
        matches!(self, Theme::Custom(_))
    }

    /// Get the custom theme if this is one
    fn as_custom(&self) -> Option<&CustomTheme> {
        match self {
            Theme::Custom(t) => Some(t),
            _ => None,
        }
    }

    /// Get mutable custom theme if this is one
    fn as_custom_mut(&mut self) -> Option<&mut CustomTheme> {
        match self {
            Theme::Custom(t) => Some(t),
            _ => None,
        }
    }
}

/// Categories for the theme editor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeEditorCategory {
    #[default]
    Backgrounds,
    Text,
    Widgets,
    Selection,
    Borders,
    Spacing,
}

impl ThemeEditorCategory {
    fn label(&self) -> &'static str {
        match self {
            ThemeEditorCategory::Backgrounds => "Backgrounds",
            ThemeEditorCategory::Text => "Text",
            ThemeEditorCategory::Widgets => "Widgets",
            ThemeEditorCategory::Selection => "Selection",
            ThemeEditorCategory::Borders => "Borders & Rounding",
            ThemeEditorCategory::Spacing => "Spacing & Layout",
        }
    }

    fn all() -> &'static [ThemeEditorCategory] {
        &[
            ThemeEditorCategory::Backgrounds,
            ThemeEditorCategory::Text,
            ThemeEditorCategory::Widgets,
            ThemeEditorCategory::Selection,
            ThemeEditorCategory::Borders,
            ThemeEditorCategory::Spacing,
        ]
    }
}

/// Context/scope where a keybinding is active
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum KeyContext {
    /// Works everywhere in the application
    #[default]
    Global,
    /// Only in the requirements list panel (not when editing text)
    RequirementsList,
    /// Only when viewing requirement details
    DetailView,
    /// Only when in add/edit form
    Form,
}

impl KeyContext {
    fn label(&self) -> &'static str {
        match self {
            KeyContext::Global => "Global",
            KeyContext::RequirementsList => "Requirements List",
            KeyContext::DetailView => "Detail View",
            KeyContext::Form => "Form",
        }
    }

    fn all() -> &'static [KeyContext] {
        &[
            KeyContext::Global,
            KeyContext::RequirementsList,
            KeyContext::DetailView,
            KeyContext::Form,
        ]
    }
}

/// Actions that can be bound to keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyAction {
    NavigateUp,
    NavigateDown,
    Edit,
    ToggleExpand,
    Save,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    CycleTheme,
    NewRequirement,
}

impl KeyAction {
    fn label(&self) -> &'static str {
        match self {
            KeyAction::NavigateUp => "Navigate Up",
            KeyAction::NavigateDown => "Navigate Down",
            KeyAction::Edit => "Edit Requirement",
            KeyAction::ToggleExpand => "Toggle Expand/Collapse",
            KeyAction::Save => "Save",
            KeyAction::ZoomIn => "Zoom In",
            KeyAction::ZoomOut => "Zoom Out",
            KeyAction::ZoomReset => "Reset Zoom",
            KeyAction::CycleTheme => "Cycle Theme",
            KeyAction::NewRequirement => "New Requirement",
        }
    }

    /// Returns the default context for this action
    fn default_context(&self) -> KeyContext {
        match self {
            KeyAction::NavigateUp => KeyContext::RequirementsList,
            KeyAction::NavigateDown => KeyContext::RequirementsList,
            KeyAction::Edit => KeyContext::RequirementsList,
            KeyAction::ToggleExpand => KeyContext::RequirementsList,
            KeyAction::Save => KeyContext::Form,
            KeyAction::ZoomIn => KeyContext::Global,
            KeyAction::ZoomOut => KeyContext::Global,
            KeyAction::ZoomReset => KeyContext::Global,
            KeyAction::CycleTheme => KeyContext::Global,
            KeyAction::NewRequirement => KeyContext::Global,
        }
    }

    fn all() -> &'static [KeyAction] {
        &[
            KeyAction::NavigateUp,
            KeyAction::NavigateDown,
            KeyAction::Edit,
            KeyAction::ToggleExpand,
            KeyAction::Save,
            KeyAction::ZoomIn,
            KeyAction::ZoomOut,
            KeyAction::ZoomReset,
            KeyAction::CycleTheme,
            KeyAction::NewRequirement,
        ]
    }
}

/// A key binding with optional modifiers and context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key_name: String, // Store as string for serialization
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    #[serde(default)]
    pub context: KeyContext,
}

impl KeyBinding {
    fn new(key: egui::Key, context: KeyContext) -> Self {
        Self {
            key_name: key_to_string(key).to_string(),
            ctrl: false,
            shift: false,
            alt: false,
            context,
        }
    }

    fn with_ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    fn with_shift(mut self) -> Self {
        self.shift = true;
        self
    }

    fn key(&self) -> Option<egui::Key> {
        string_to_key(&self.key_name)
    }

    fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(&self.key_name);
        parts.join("+")
    }

    /// Check if the key combination is pressed (does not check context)
    fn key_matches(&self, ctx: &egui::Context) -> bool {
        let Some(key) = self.key() else { return false };
        ctx.input(|i| {
            let modifiers_match = i.modifiers.ctrl == self.ctrl
                && i.modifiers.shift == self.shift
                && i.modifiers.alt == self.alt;
            modifiers_match && i.key_pressed(key)
        })
    }

    /// Check if the binding matches given the current app context
    fn matches(&self, egui_ctx: &egui::Context, current_context: KeyContext) -> bool {
        // Global bindings work everywhere
        // RequirementsList bindings also work in DetailView since both show the list panel
        // Otherwise, context must match
        let context_matches = self.context == KeyContext::Global
            || self.context == current_context
            || (self.context == KeyContext::RequirementsList
                && current_context == KeyContext::DetailView);
        context_matches && self.key_matches(egui_ctx)
    }
}

fn key_to_string(key: egui::Key) -> &'static str {
    match key {
        egui::Key::ArrowUp => "Up",
        egui::Key::ArrowDown => "Down",
        egui::Key::ArrowLeft => "Left",
        egui::Key::ArrowRight => "Right",
        egui::Key::Enter => "Enter",
        egui::Key::Space => "Space",
        egui::Key::Tab => "Tab",
        egui::Key::Escape => "Escape",
        egui::Key::Backspace => "Backspace",
        egui::Key::Delete => "Delete",
        egui::Key::Home => "Home",
        egui::Key::End => "End",
        egui::Key::PageUp => "PageUp",
        egui::Key::PageDown => "PageDown",
        egui::Key::Plus => "Plus",
        egui::Key::Minus => "Minus",
        egui::Key::Equals => "Equals",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::A => "A",
        egui::Key::B => "B",
        egui::Key::C => "C",
        egui::Key::D => "D",
        egui::Key::E => "E",
        egui::Key::F => "F",
        egui::Key::G => "G",
        egui::Key::H => "H",
        egui::Key::I => "I",
        egui::Key::J => "J",
        egui::Key::K => "K",
        egui::Key::L => "L",
        egui::Key::M => "M",
        egui::Key::N => "N",
        egui::Key::O => "O",
        egui::Key::P => "P",
        egui::Key::Q => "Q",
        egui::Key::R => "R",
        egui::Key::S => "S",
        egui::Key::T => "T",
        egui::Key::U => "U",
        egui::Key::V => "V",
        egui::Key::W => "W",
        egui::Key::X => "X",
        egui::Key::Y => "Y",
        egui::Key::Z => "Z",
        egui::Key::F1 => "F1",
        egui::Key::F2 => "F2",
        egui::Key::F3 => "F3",
        egui::Key::F4 => "F4",
        egui::Key::F5 => "F5",
        egui::Key::F6 => "F6",
        egui::Key::F7 => "F7",
        egui::Key::F8 => "F8",
        egui::Key::F9 => "F9",
        egui::Key::F10 => "F10",
        egui::Key::F11 => "F11",
        egui::Key::F12 => "F12",
        _ => "?",
    }
}

fn string_to_key(s: &str) -> Option<egui::Key> {
    match s {
        "Up" => Some(egui::Key::ArrowUp),
        "Down" => Some(egui::Key::ArrowDown),
        "Left" => Some(egui::Key::ArrowLeft),
        "Right" => Some(egui::Key::ArrowRight),
        "Enter" => Some(egui::Key::Enter),
        "Space" => Some(egui::Key::Space),
        "Tab" => Some(egui::Key::Tab),
        "Escape" => Some(egui::Key::Escape),
        "Backspace" => Some(egui::Key::Backspace),
        "Delete" => Some(egui::Key::Delete),
        "Home" => Some(egui::Key::Home),
        "End" => Some(egui::Key::End),
        "PageUp" => Some(egui::Key::PageUp),
        "PageDown" => Some(egui::Key::PageDown),
        "Plus" => Some(egui::Key::Plus),
        "Minus" => Some(egui::Key::Minus),
        "Equals" => Some(egui::Key::Equals),
        "0" => Some(egui::Key::Num0),
        "1" => Some(egui::Key::Num1),
        "2" => Some(egui::Key::Num2),
        "3" => Some(egui::Key::Num3),
        "4" => Some(egui::Key::Num4),
        "5" => Some(egui::Key::Num5),
        "6" => Some(egui::Key::Num6),
        "7" => Some(egui::Key::Num7),
        "8" => Some(egui::Key::Num8),
        "9" => Some(egui::Key::Num9),
        "A" => Some(egui::Key::A),
        "B" => Some(egui::Key::B),
        "C" => Some(egui::Key::C),
        "D" => Some(egui::Key::D),
        "E" => Some(egui::Key::E),
        "F" => Some(egui::Key::F),
        "G" => Some(egui::Key::G),
        "H" => Some(egui::Key::H),
        "I" => Some(egui::Key::I),
        "J" => Some(egui::Key::J),
        "K" => Some(egui::Key::K),
        "L" => Some(egui::Key::L),
        "M" => Some(egui::Key::M),
        "N" => Some(egui::Key::N),
        "O" => Some(egui::Key::O),
        "P" => Some(egui::Key::P),
        "Q" => Some(egui::Key::Q),
        "R" => Some(egui::Key::R),
        "S" => Some(egui::Key::S),
        "T" => Some(egui::Key::T),
        "U" => Some(egui::Key::U),
        "V" => Some(egui::Key::V),
        "W" => Some(egui::Key::W),
        "X" => Some(egui::Key::X),
        "Y" => Some(egui::Key::Y),
        "Z" => Some(egui::Key::Z),
        "F1" => Some(egui::Key::F1),
        "F2" => Some(egui::Key::F2),
        "F3" => Some(egui::Key::F3),
        "F4" => Some(egui::Key::F4),
        "F5" => Some(egui::Key::F5),
        "F6" => Some(egui::Key::F6),
        "F7" => Some(egui::Key::F7),
        "F8" => Some(egui::Key::F8),
        "F9" => Some(egui::Key::F9),
        "F10" => Some(egui::Key::F10),
        "F11" => Some(egui::Key::F11),
        "F12" => Some(egui::Key::F12),
        _ => None,
    }
}

/// Collection of key bindings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    pub bindings: HashMap<KeyAction, KeyBinding>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        // Use default_context() for each action
        bindings.insert(
            KeyAction::NavigateUp,
            KeyBinding::new(egui::Key::ArrowUp, KeyAction::NavigateUp.default_context()),
        );
        bindings.insert(
            KeyAction::NavigateDown,
            KeyBinding::new(
                egui::Key::ArrowDown,
                KeyAction::NavigateDown.default_context(),
            ),
        );
        bindings.insert(
            KeyAction::Edit,
            KeyBinding::new(egui::Key::Enter, KeyAction::Edit.default_context()),
        );
        bindings.insert(
            KeyAction::ToggleExpand,
            KeyBinding::new(egui::Key::Space, KeyAction::ToggleExpand.default_context()),
        );
        bindings.insert(
            KeyAction::Save,
            KeyBinding::new(egui::Key::S, KeyAction::Save.default_context()).with_ctrl(),
        );
        bindings.insert(
            KeyAction::ZoomIn,
            KeyBinding::new(egui::Key::Plus, KeyAction::ZoomIn.default_context())
                .with_ctrl()
                .with_shift(),
        );
        bindings.insert(
            KeyAction::ZoomOut,
            KeyBinding::new(egui::Key::Minus, KeyAction::ZoomOut.default_context()).with_ctrl(),
        );
        bindings.insert(
            KeyAction::ZoomReset,
            KeyBinding::new(egui::Key::Num0, KeyAction::ZoomReset.default_context()).with_ctrl(),
        );
        bindings.insert(
            KeyAction::CycleTheme,
            KeyBinding::new(egui::Key::T, KeyAction::CycleTheme.default_context()).with_ctrl(),
        );
        bindings.insert(
            KeyAction::NewRequirement,
            KeyBinding::new(egui::Key::N, KeyAction::NewRequirement.default_context()).with_ctrl(),
        );
        Self { bindings }
    }
}

impl KeyBindings {
    #[allow(dead_code)]
    fn get(&self, action: KeyAction) -> Option<&KeyBinding> {
        self.bindings.get(&action)
    }

    /// Check if an action's keybinding is pressed in the given context
    fn is_pressed(
        &self,
        action: KeyAction,
        egui_ctx: &egui::Context,
        current_context: KeyContext,
    ) -> bool {
        self.bindings
            .get(&action)
            .map(|binding| binding.matches(egui_ctx, current_context))
            .unwrap_or(false)
    }

    /// Check if an action's keybinding is pressed (ignores context - for key capture)
    #[allow(dead_code)]
    fn is_key_pressed(&self, action: KeyAction, egui_ctx: &egui::Context) -> bool {
        self.bindings
            .get(&action)
            .map(|binding| binding.key_matches(egui_ctx))
            .unwrap_or(false)
    }
}

/// Icon configuration for status indicators
/// Maps status keywords to display icons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusIconConfig {
    /// Map of status keyword (lowercase) to icon string
    pub icons: std::collections::HashMap<String, String>,
    /// Default icon for unknown statuses
    pub default_icon: String,
}

impl Default for StatusIconConfig {
    fn default() -> Self {
        let mut icons = std::collections::HashMap::new();
        // Default ASCII-based markers
        icons.insert("completed".to_string(), "[x]".to_string());
        icons.insert("done".to_string(), "[x]".to_string());
        icons.insert("rejected".to_string(), "[-]".to_string());
        icons.insert("closed".to_string(), "[-]".to_string());
        icons.insert("draft".to_string(), "[ ]".to_string());
        icons.insert("review".to_string(), "[?]".to_string());
        icons.insert("approved".to_string(), "[+]".to_string());
        icons.insert("ready".to_string(), "[+]".to_string());
        icons.insert("progress".to_string(), "[~]".to_string());
        icons.insert("implement".to_string(), "[~]".to_string());
        icons.insert("verified".to_string(), "[x]".to_string());
        icons.insert("backlog".to_string(), "[.]".to_string());
        icons.insert("open".to_string(), "[!]".to_string());
        icons.insert("confirmed".to_string(), "[!]".to_string());
        icons.insert("fixed".to_string(), "[x]".to_string());
        Self {
            icons,
            default_icon: "[*]".to_string(),
        }
    }
}

impl StatusIconConfig {
    /// Get the icon for a status string
    pub fn get_icon(&self, status: &str) -> &str {
        let status_lower = status.to_lowercase();
        // Check for exact match first
        if let Some(icon) = self.icons.get(&status_lower) {
            return icon;
        }
        // Check if any key is contained in the status
        for (key, icon) in &self.icons {
            if status_lower.contains(key) {
                return icon;
            }
        }
        &self.default_icon
    }
}

/// Icon configuration for priority indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityIconConfig {
    /// Map of priority keyword (lowercase) to icon string
    pub icons: std::collections::HashMap<String, String>,
    /// Default icon for unknown priorities
    pub default_icon: String,
}

impl Default for PriorityIconConfig {
    fn default() -> Self {
        let mut icons = std::collections::HashMap::new();
        icons.insert("high".to_string(), "!!!".to_string());
        icons.insert("critical".to_string(), "!!!".to_string());
        icons.insert("medium".to_string(), "!!".to_string());
        icons.insert("low".to_string(), "!".to_string());
        icons.insert("trivial".to_string(), ".".to_string());
        Self {
            icons,
            default_icon: "".to_string(),
        }
    }
}

impl PriorityIconConfig {
    /// Get the icon for a priority string
    pub fn get_icon(&self, priority: &str) -> &str {
        let priority_lower = priority.to_lowercase();
        if let Some(icon) = self.icons.get(&priority_lower) {
            return icon;
        }
        for (key, icon) in &self.icons {
            if priority_lower.contains(key) {
                return icon;
            }
        }
        &self.default_icon
    }
}

/// User settings for the GUI application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    /// User's full name
    pub name: String,
    /// User's email address
    pub email: String,
    /// User's nickname/handle for @mentions in comments
    pub handle: String,
    /// Base font size in points
    #[serde(default = "default_font_size")]
    pub base_font_size: f32,
    /// UI title heading level (1-6, default 3)
    #[serde(default = "default_ui_heading_level")]
    pub ui_heading_level: u8,
    /// Preferred view perspective
    #[serde(default)]
    pub preferred_perspective: Perspective,
    /// Color theme
    #[serde(default)]
    pub theme: Theme,
    /// Key bindings
    #[serde(default)]
    pub keybindings: KeyBindings,
    /// Saved view presets
    #[serde(default)]
    pub view_presets: Vec<ViewPreset>,
    /// Saved custom themes
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
    /// Show status icons in requirements list
    #[serde(default)]
    pub show_status_icons: bool,
    /// Status icon configuration
    #[serde(default)]
    pub status_icons: StatusIconConfig,
    /// Priority icon configuration
    #[serde(default)]
    pub priority_icons: PriorityIconConfig,
}

fn default_font_size() -> f32 {
    DEFAULT_FONT_SIZE
}

fn default_ui_heading_level() -> u8 {
    5 // H5 by default
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            email: String::new(),
            handle: String::new(),
            base_font_size: DEFAULT_FONT_SIZE,
            ui_heading_level: default_ui_heading_level(),
            preferred_perspective: Perspective::default(),
            theme: Theme::default(),
            keybindings: KeyBindings::default(),
            view_presets: Vec::new(),
            custom_themes: Vec::new(),
            show_status_icons: false,
            status_icons: StatusIconConfig::default(),
            priority_icons: PriorityIconConfig::default(),
        }
    }
}

impl UserSettings {
    /// Get the default settings file path
    fn settings_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".requirements_gui_settings.yaml")
    }

    /// Load settings from file, or return defaults if not found
    pub fn load() -> Self {
        let path = Self::settings_path();
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(settings) = serde_yaml::from_str(&contents) {
                    return settings;
                }
            }
        }
        Self::default()
    }

    /// Save settings to file
    pub fn save(&self) -> Result<(), String> {
        let path = Self::settings_path();
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        std::fs::write(&path, yaml).map_err(|e| format!("Failed to write settings file: {}", e))?;
        Ok(())
    }

    /// Get the display name for use in comments/history
    /// Returns handle if set, otherwise name, otherwise "Unknown User"
    pub fn display_name(&self) -> String {
        if !self.handle.is_empty() {
            self.handle.clone()
        } else if !self.name.is_empty() {
            self.name.clone()
        } else {
            "Unknown User".to_string()
        }
    }
}

#[derive(Default, PartialEq, Clone)]
enum DetailTab {
    #[default]
    Description,
    Comments,
    Links,
    History,
}

#[derive(Default, PartialEq, Clone)]
enum SettingsTab {
    #[default]
    User,
    Appearance,
    Keybindings,
    IDs,
    Relationships,
    Reactions,
    TypeDefinitions,
    Users,
    Database,
}

#[derive(Default, PartialEq, Clone, Copy)]
enum FilterTab {
    #[default]
    Root,
    Children,
}

/// What fields to include in text search
#[derive(Default, PartialEq, Clone, Copy)]
struct SearchScope {
    title: bool,
    description: bool,
    comments: bool,
    spec_id: bool,
}

impl SearchScope {
    /// Default scope searches everything
    fn all() -> Self {
        Self {
            title: true,
            description: true,
            comments: true,
            spec_id: true,
        }
    }

    /// Check if all scopes are enabled (for "Everything" display)
    fn is_all(&self) -> bool {
        self.title && self.description && self.comments && self.spec_id
    }

    /// Check if no scopes are enabled
    fn is_none(&self) -> bool {
        !self.title && !self.description && !self.comments && !self.spec_id
    }
}

#[derive(Default, Debug, PartialEq, Clone)]
enum View {
    #[default]
    List,
    Detail,
    Add,
    Edit,
}

/// Perspective defines how requirements are organized in the list
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub(crate) enum Perspective {
    /// Simple flat list of all requirements
    Flat,
    /// Tree view based on Parent/Child relationships (default)
    #[default]
    ParentChild,
    /// Tree view based on Verifies/VerifiedBy relationships
    Verification,
    /// Tree view based on References relationships
    References,
}

impl Perspective {
    fn label(&self) -> &'static str {
        match self {
            Perspective::Flat => "Flat List",
            Perspective::ParentChild => "Parent/Child",
            Perspective::Verification => "Verification",
            Perspective::References => "References",
        }
    }

    /// Get the relationship types used for this perspective
    fn relationship_types(&self) -> Option<(RelationshipType, RelationshipType)> {
        match self {
            Perspective::Flat => None,
            Perspective::ParentChild => Some((RelationshipType::Parent, RelationshipType::Child)),
            Perspective::Verification => {
                Some((RelationshipType::Verifies, RelationshipType::VerifiedBy))
            }
            Perspective::References => {
                Some((RelationshipType::References, RelationshipType::References))
            }
        }
    }
}

/// Direction for viewing relationship hierarchies
#[derive(Debug, Default, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub(crate) enum PerspectiveDirection {
    /// View from parent/source to children/targets
    #[default]
    TopDown,
    /// View from children/targets to parents/sources
    BottomUp,
}

impl PerspectiveDirection {
    fn label(&self) -> &'static str {
        match self {
            PerspectiveDirection::TopDown => "Top-down",
            PerspectiveDirection::BottomUp => "Bottom-up",
        }
    }
}

/// A saved view preset combining perspective, direction, and filters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewPreset {
    /// User-defined name for the preset
    pub name: String,
    /// The perspective (Flat, ParentChild, etc.)
    pub perspective: Perspective,
    /// Direction for tree views
    pub direction: PerspectiveDirection,
    /// Root filter by requirement types (empty = all)
    pub filter_types: Vec<String>,
    /// Root filter by features (empty = all)
    pub filter_features: Vec<String>,
    /// Root filter by ID prefixes (empty = all)
    #[serde(default)]
    pub filter_prefixes: Vec<String>,
    /// Child filter by requirement types (empty = all)
    #[serde(default)]
    pub child_filter_types: Vec<String>,
    /// Child filter by features (empty = all)
    #[serde(default)]
    pub child_filter_features: Vec<String>,
    /// Child filter by ID prefixes (empty = all)
    #[serde(default)]
    pub child_filter_prefixes: Vec<String>,
    /// Whether children use same filters as root
    #[serde(default = "default_true")]
    pub children_same_as_root: bool,
}

fn default_true() -> bool {
    true
}

impl ViewPreset {
    /// Create a new preset with the given name and current settings
    fn new(
        name: String,
        perspective: Perspective,
        direction: PerspectiveDirection,
        filter_types: &HashSet<RequirementType>,
        filter_features: &HashSet<String>,
        filter_prefixes: &HashSet<String>,
        child_filter_types: &HashSet<RequirementType>,
        child_filter_features: &HashSet<String>,
        child_filter_prefixes: &HashSet<String>,
        children_same_as_root: bool,
    ) -> Self {
        Self {
            name,
            perspective,
            direction,
            filter_types: filter_types.iter().map(|t| format!("{:?}", t)).collect(),
            filter_features: filter_features.iter().cloned().collect(),
            filter_prefixes: filter_prefixes.iter().cloned().collect(),
            child_filter_types: child_filter_types
                .iter()
                .map(|t| format!("{:?}", t))
                .collect(),
            child_filter_features: child_filter_features.iter().cloned().collect(),
            child_filter_prefixes: child_filter_prefixes.iter().cloned().collect(),
            children_same_as_root,
        }
    }

    /// Get filter_types as HashSet<RequirementType>
    fn get_filter_types(&self) -> HashSet<RequirementType> {
        Self::parse_types(&self.filter_types)
    }

    /// Get child_filter_types as HashSet<RequirementType>
    fn get_child_filter_types(&self) -> HashSet<RequirementType> {
        Self::parse_types(&self.child_filter_types)
    }

    fn parse_types(types: &[String]) -> HashSet<RequirementType> {
        types
            .iter()
            .filter_map(|s| match s.as_str() {
                "Functional" => Some(RequirementType::Functional),
                "NonFunctional" => Some(RequirementType::NonFunctional),
                "System" => Some(RequirementType::System),
                "User" => Some(RequirementType::User),
                "ChangeRequest" => Some(RequirementType::ChangeRequest),
                _ => None,
            })
            .collect()
    }

    /// Get filter_features as HashSet<String>
    fn get_filter_features(&self) -> HashSet<String> {
        self.filter_features.iter().cloned().collect()
    }

    /// Get child_filter_features as HashSet<String>
    fn get_child_filter_features(&self) -> HashSet<String> {
        self.child_filter_features.iter().cloned().collect()
    }

    /// Get filter_prefixes as HashSet<String>
    fn get_filter_prefixes(&self) -> HashSet<String> {
        self.filter_prefixes.iter().cloned().collect()
    }

    /// Get child_filter_prefixes as HashSet<String>
    fn get_child_filter_prefixes(&self) -> HashSet<String> {
        self.child_filter_prefixes.iter().cloned().collect()
    }
}

pub struct RequirementsApp {
    storage: Storage,
    store: RequirementsStore,
    current_view: View,
    selected_idx: Option<usize>,
    filter_text: String,
    search_scope: SearchScope,
    active_tab: DetailTab,

    // Form state
    form_title: String,
    form_description: String,
    form_status: RequirementStatus,
    form_status_string: String, // Status as string (for custom type statuses)
    form_custom_fields: HashMap<String, String>, // Custom field values
    form_priority: RequirementPriority,
    form_type: RequirementType,
    form_owner: String,
    form_feature: String,
    form_tags: String,
    form_prefix: String, // Optional prefix override (uppercase letters only)
    form_parent_id: Option<Uuid>, // Parent to link new requirement to
    focus_description: bool, // Request focus on description field when entering Edit view

    // Messages
    message: Option<(String, bool)>, // (message, is_error)

    // Comment state
    comment_author: String,
    comment_content: String,
    show_add_comment: bool,
    reply_to_comment: Option<Uuid>, // Parent comment ID for replies
    collapsed_comments: HashMap<Uuid, bool>, // Track which comments are collapsed
    #[allow(dead_code)]
    edit_comment_id: Option<Uuid>,

    // Pending operations (to avoid borrow checker issues)
    pending_delete: Option<usize>,
    pending_view_change: Option<View>,
    pending_save: bool, // Save triggered by keybinding
    pending_comment_add: Option<(String, String, Option<Uuid>)>, // (author, content, parent_id)
    pending_comment_delete: Option<Uuid>,
    pending_reaction_toggle: Option<(Uuid, String)>, // (comment_id, reaction_name)
    show_reaction_picker: Option<Uuid>,              // Comment ID to show reaction picker for
    scroll_to_requirement: Option<Uuid>,             // Requirement ID to scroll into view

    // Settings
    user_settings: UserSettings,
    show_settings_dialog: bool,
    settings_tab: SettingsTab,
    settings_form_name: String,
    settings_form_email: String,
    settings_form_handle: String,
    settings_form_font_size: f32,
    settings_form_ui_heading_level: u8,
    settings_form_perspective: Perspective,
    settings_form_theme: Theme,
    settings_form_keybindings: KeyBindings,
    capturing_key_for: Option<KeyAction>, // Which action we're capturing a key for
    settings_form_show_status_icons: bool,
    settings_form_status_icons: StatusIconConfig,
    settings_form_priority_icons: PriorityIconConfig,
    show_icon_editor: bool, // Whether to show the icon editor dialog
    icon_editor_new_keyword: String, // For adding new status/priority keywords
    icon_editor_new_icon: String, // Icon for new keyword
    show_symbol_picker: bool, // Whether to show the symbol picker popup
    symbol_picker_target: Option<String>, // Which field the symbol picker is targeting

    // Original appearance settings for Cancel reversion (live preview support)
    original_appearance_theme: Theme,
    original_appearance_font_size: f32,
    original_appearance_ui_heading_level: u8,
    original_appearance_show_status_icons: bool,
    original_appearance_status_icons: StatusIconConfig,
    original_appearance_priority_icons: PriorityIconConfig,

    // Project settings form fields
    settings_form_id_format: IdFormat,
    settings_form_numbering: NumberingStrategy,
    settings_form_digits: u8,
    show_migration_dialog: bool,
    pending_migration: Option<(IdFormat, NumberingStrategy, u8)>, // Format, Numbering, Digits

    // Theme editor
    show_theme_editor: bool,
    theme_editor_theme: CustomTheme,   // Working copy being edited
    theme_editor_category: ThemeEditorCategory, // Currently selected category
    theme_editor_original_theme: Theme, // Original theme before editing (for Cancel)

    // User management
    show_user_form: bool,
    editing_user_id: Option<Uuid>,
    user_form_name: String,
    user_form_email: String,
    user_form_handle: String,
    show_archived_users: bool,

    // Relationships view
    show_recursive_relationships: bool, // Toggle for recursive tree view
    relationship_tree_collapsed: HashMap<(Uuid, Uuid), bool>, // Track collapsed relationship tree nodes (source_id, target_id)

    // Font size (runtime, can differ from saved base)
    current_font_size: f32,

    // Perspective and filtering
    perspective: Perspective,
    perspective_direction: PerspectiveDirection,
    filter_types: HashSet<RequirementType>, // Root filter: Empty = show all
    filter_features: HashSet<String>,       // Root filter: Empty = show all
    filter_prefixes: HashSet<String>,       // Root filter: Empty = show all
    filter_statuses: HashSet<RequirementStatus>, // Root filter: Empty = show all
    filter_priorities: HashSet<RequirementPriority>, // Root filter: Empty = show all
    // Child filters (when children_same_as_root is false)
    child_filter_types: HashSet<RequirementType>,
    child_filter_features: HashSet<String>,
    child_filter_prefixes: HashSet<String>,
    child_filter_statuses: HashSet<RequirementStatus>,
    child_filter_priorities: HashSet<RequirementPriority>,
    children_same_as_root: bool, // When true, children use same filters as root
    filter_tab: FilterTab,       // Which filter tab is active (Root or Children)
    tree_collapsed: HashMap<Uuid, bool>, // Track collapsed tree nodes
    show_filter_panel: bool,     // Toggle filter panel visibility
    show_archived: bool,         // Whether to show archived requirements
    show_filtered_parents: bool, // Show greyed-out parents of filtered items in tree view

    // Drag and drop for relationships
    drag_source: Option<usize>, // Index of requirement being dragged
    drop_target: Option<usize>, // Index of requirement being hovered over
    pending_relationship: Option<(usize, usize)>, // (source_idx, target_idx) to create relationship
    drag_scroll_delta: f32,     // Accumulated scroll delta during drag (for auto-scroll)

    // Markdown rendering
    markdown_cache: CommonMarkCache,
    show_description_preview: bool, // Toggle preview mode in edit form

    // Left panel state
    left_panel_collapsed: bool, // Whether left panel is manually collapsed

    // Relationship definition editing
    editing_rel_def: Option<String>, // Name of relationship def being edited (None = adding new)
    rel_def_form_name: String,
    rel_def_form_display_name: String,
    rel_def_form_description: String,
    rel_def_form_inverse: String,
    rel_def_form_symmetric: bool,
    rel_def_form_cardinality: Cardinality,
    rel_def_form_source_types: String, // Comma-separated
    rel_def_form_target_types: String, // Comma-separated
    rel_def_form_color: String,
    show_rel_def_form: bool,

    // View presets
    active_preset: Option<String>, // Name of currently active preset (None = custom/unsaved)
    show_save_preset_dialog: bool, // Show the save preset dialog
    preset_name_input: String,     // Name input for new preset
    show_delete_preset_confirm: Option<String>, // Name of preset to confirm deletion

    // Keybinding context
    current_key_context: KeyContext, // Current context for keybinding checks

    // Reaction definition editing
    editing_reaction_def: Option<String>, // Name of reaction def being edited (None = adding new)
    reaction_def_form_name: String,
    reaction_def_form_emoji: String,
    reaction_def_form_label: String,
    reaction_def_form_description: String,
    show_reaction_def_form: bool,

    // Prefix management
    new_prefix_input: String, // Input for adding new allowed prefix

    // Type definition editing
    editing_type_def: Option<String>, // Name of type def being edited (None = adding new)
    type_def_form_name: String,
    type_def_form_display_name: String,
    type_def_form_description: String,
    type_def_form_prefix: String,
    type_def_form_statuses: Vec<String>, // Editable list of statuses
    type_def_form_fields: Vec<CustomFieldDefinition>, // Editable list of custom fields
    show_type_def_form: bool,
    new_status_input: String, // Input for adding new status
    // Custom field form (for adding/editing fields within type)
    editing_field_idx: Option<usize>, // Index of field being edited (None = adding new)
    field_form_name: String,
    field_form_label: String,
    field_form_type: CustomFieldType,
    field_form_required: bool,
    field_form_options: String, // Comma-separated for Select type
    field_form_default: String,
    show_field_form: bool,

    // URL link editing
    show_url_form: bool,
    editing_url_id: Option<Uuid>, // None = adding new, Some = editing existing
    url_form_url: String,
    url_form_title: String,
    url_form_description: String,
    url_verification_status: Option<(bool, String)>, // (success, message)
    url_verification_in_progress: bool,

    // Markdown help modal
    show_markdown_help: bool,

    // Context menu state - stores selection before right-click clears it
    last_text_selection: Option<TextSelection>,

    // Project management
    show_new_project_dialog: bool,
    new_project_dir: String,
    new_project_name: String,
    new_project_title: String,
    new_project_description: String,
    new_project_template: String, // "current" or template name
    new_project_include_users: bool,
    show_switch_project_dialog: bool,
    available_projects: Vec<(String, String, String)>, // (name, path, description)

    // Form cancel confirmation
    show_cancel_confirm_dialog: bool, // Show confirmation dialog when canceling with unsaved changes
    original_form_title: String,      // Original values when entering Add/Edit mode
    original_form_description: String,
    original_form_status_string: String,
    original_form_priority: RequirementPriority,
    original_form_type: RequirementType,
    original_form_owner: String,
    original_form_feature: String,
    original_form_tags: String,
    original_form_prefix: String,
    original_form_custom_fields: HashMap<String, String>,
}

/// Stores text selection state for context menu operations.
///
/// This is part of the workaround for egui clearing selection on right-click.
/// See the TEXT EDIT CONTEXT MENU IMPLEMENTATION documentation at the top of this file.
///
/// The selection is captured continuously while a TextEdit has focus, and stored
/// here so it's available when the context menu opens (after egui has cleared the
/// actual selection).
#[derive(Clone, Default)]
struct TextSelection {
    /// The selected text content
    text: String,
    /// Start index (in chars, not bytes)
    start: usize,
    /// End index (in chars, not bytes)
    end: usize,
    /// Which widget this selection came from (to avoid using stale selections)
    widget_id: Option<egui::Id>,
}

impl RequirementsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Configure fonts with better Unicode support
        Self::configure_fonts(&cc.egui_ctx);

        // Configure heading styles for markdown rendering
        // egui_commonmark uses named text styles: "Heading", "Heading2", "Heading3", etc.
        {
            let mut style = (*cc.egui_ctx.style()).clone();
            let base_size = style
                .text_styles
                .get(&egui::TextStyle::Body)
                .map(|f| f.size)
                .unwrap_or(14.0);

            // Set distinct sizes for each heading level
            style.text_styles.insert(
                egui::TextStyle::Name("Heading".into()),
                egui::FontId::new(base_size * 1.8, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Name("Heading2".into()),
                egui::FontId::new(base_size * 1.5, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Name("Heading3".into()),
                egui::FontId::new(base_size * 1.25, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Name("Heading4".into()),
                egui::FontId::new(base_size * 1.1, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Name("Heading5".into()),
                egui::FontId::new(base_size * 1.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Name("Heading6".into()),
                egui::FontId::new(base_size * 0.9, egui::FontFamily::Proportional),
            );
            cc.egui_ctx.set_style(style);
        }

        let requirements_path = determine_requirements_path(None)
            .unwrap_or_else(|_| std::path::PathBuf::from("requirements.yaml"));

        let storage = Storage::new(requirements_path);
        let store = storage.load().unwrap_or_else(|_| RequirementsStore::new());
        let user_settings = UserSettings::load();

        // Extract project settings before store is moved
        let initial_id_format = store.id_config.format.clone();
        let initial_numbering = store.id_config.numbering.clone();
        let initial_digits = store.id_config.digits;

        // Apply saved preferences
        let initial_font_size = user_settings.base_font_size;
        let initial_perspective = user_settings.preferred_perspective.clone();

        Self {
            storage,
            store,
            current_view: View::List,
            selected_idx: None,
            filter_text: String::new(),
            search_scope: SearchScope::all(),
            active_tab: DetailTab::Description,
            form_title: String::new(),
            form_description: String::new(),
            form_status: RequirementStatus::Draft,
            form_status_string: String::from("Draft"),
            form_custom_fields: HashMap::new(),
            form_priority: RequirementPriority::Medium,
            form_type: RequirementType::Functional,
            form_owner: String::new(),
            form_feature: String::from("Uncategorized"),
            form_tags: String::new(),
            form_prefix: String::new(),
            form_parent_id: None,
            focus_description: false,
            message: None,
            comment_author: String::new(),
            comment_content: String::new(),
            show_add_comment: false,
            reply_to_comment: None,
            collapsed_comments: HashMap::new(),
            edit_comment_id: None,
            pending_delete: None,
            pending_view_change: None,
            pending_save: false,
            pending_comment_add: None,
            pending_comment_delete: None,
            pending_reaction_toggle: None,
            show_reaction_picker: None,
            scroll_to_requirement: None,
            current_font_size: initial_font_size,
            user_settings,
            show_settings_dialog: false,
            settings_tab: SettingsTab::default(),
            settings_form_name: String::new(),
            settings_form_email: String::new(),
            settings_form_handle: String::new(),
            settings_form_font_size: DEFAULT_FONT_SIZE,
            settings_form_ui_heading_level: default_ui_heading_level(),
            settings_form_perspective: Perspective::default(),
            settings_form_theme: Theme::default(),
            settings_form_keybindings: KeyBindings::default(),
            capturing_key_for: None,
            settings_form_show_status_icons: false,
            settings_form_status_icons: StatusIconConfig::default(),
            settings_form_priority_icons: PriorityIconConfig::default(),
            show_icon_editor: false,
            icon_editor_new_keyword: String::new(),
            icon_editor_new_icon: String::new(),
            show_symbol_picker: false,
            symbol_picker_target: None,
            original_appearance_theme: Theme::default(),
            original_appearance_font_size: DEFAULT_FONT_SIZE,
            original_appearance_ui_heading_level: default_ui_heading_level(),
            original_appearance_show_status_icons: false,
            original_appearance_status_icons: StatusIconConfig::default(),
            original_appearance_priority_icons: PriorityIconConfig::default(),
            settings_form_id_format: initial_id_format,
            settings_form_numbering: initial_numbering,
            settings_form_digits: initial_digits,
            show_migration_dialog: false,
            pending_migration: None,
            show_theme_editor: false,
            theme_editor_theme: CustomTheme::default(),
            theme_editor_category: ThemeEditorCategory::default(),
            theme_editor_original_theme: Theme::default(),
            show_user_form: false,
            editing_user_id: None,
            user_form_name: String::new(),
            user_form_email: String::new(),
            user_form_handle: String::new(),
            show_archived_users: false,
            show_recursive_relationships: false,
            relationship_tree_collapsed: HashMap::new(),
            perspective: initial_perspective,
            perspective_direction: PerspectiveDirection::default(),
            filter_types: HashSet::new(),
            filter_features: HashSet::new(),
            filter_prefixes: HashSet::new(),
            filter_statuses: HashSet::new(),
            filter_priorities: HashSet::new(),
            child_filter_types: HashSet::new(),
            child_filter_features: HashSet::new(),
            child_filter_prefixes: HashSet::new(),
            child_filter_statuses: HashSet::new(),
            child_filter_priorities: HashSet::new(),
            children_same_as_root: true,
            filter_tab: FilterTab::Root,
            tree_collapsed: HashMap::new(),
            show_filter_panel: false,
            show_archived: false,
            show_filtered_parents: true, // Default to showing parents
            drag_source: None,
            drop_target: None,
            pending_relationship: None,
            drag_scroll_delta: 0.0,
            markdown_cache: CommonMarkCache::default(),
            show_description_preview: false,
            left_panel_collapsed: false,
            editing_rel_def: None,
            rel_def_form_name: String::new(),
            rel_def_form_display_name: String::new(),
            rel_def_form_description: String::new(),
            rel_def_form_inverse: String::new(),
            rel_def_form_symmetric: false,
            rel_def_form_cardinality: Cardinality::default(),
            rel_def_form_source_types: String::new(),
            rel_def_form_target_types: String::new(),
            rel_def_form_color: String::new(),
            show_rel_def_form: false,
            active_preset: None,
            show_save_preset_dialog: false,
            preset_name_input: String::new(),
            show_delete_preset_confirm: None,
            current_key_context: KeyContext::RequirementsList,
            editing_reaction_def: None,
            reaction_def_form_name: String::new(),
            reaction_def_form_emoji: String::new(),
            reaction_def_form_label: String::new(),
            reaction_def_form_description: String::new(),
            show_reaction_def_form: false,
            new_prefix_input: String::new(),
            // Type definition editing
            editing_type_def: None,
            type_def_form_name: String::new(),
            type_def_form_display_name: String::new(),
            type_def_form_description: String::new(),
            type_def_form_prefix: String::new(),
            type_def_form_statuses: Vec::new(),
            type_def_form_fields: Vec::new(),
            show_type_def_form: false,
            new_status_input: String::new(),
            editing_field_idx: None,
            field_form_name: String::new(),
            field_form_label: String::new(),
            field_form_type: CustomFieldType::Text,
            field_form_required: false,
            field_form_options: String::new(),
            field_form_default: String::new(),
            show_field_form: false,
            // URL form
            show_url_form: false,
            editing_url_id: None,
            url_form_url: String::new(),
            url_form_title: String::new(),
            url_form_description: String::new(),
            url_verification_status: None,
            url_verification_in_progress: false,
            // Markdown help
            show_markdown_help: false,
            // Context menu
            last_text_selection: None,
            // Project management
            show_new_project_dialog: false,
            new_project_dir: String::new(),
            new_project_name: String::new(),
            new_project_title: String::new(),
            new_project_description: String::new(),
            new_project_template: "current".to_string(),
            new_project_include_users: false,
            show_switch_project_dialog: false,
            available_projects: Vec::new(),
            // Form cancel confirmation
            show_cancel_confirm_dialog: false,
            original_form_title: String::new(),
            original_form_description: String::new(),
            original_form_status_string: String::new(),
            original_form_priority: RequirementPriority::Medium,
            original_form_type: RequirementType::Functional,
            original_form_owner: String::new(),
            original_form_feature: String::new(),
            original_form_tags: String::new(),
            original_form_prefix: String::new(),
            original_form_custom_fields: HashMap::new(),
        }
    }

    /// Configure fonts with better Unicode symbol support
    fn configure_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        // Embed DejaVu Sans font at compile time for cross-platform Unicode support
        // DejaVu Sans is licensed under a free license (similar to MIT)
        // See: https://dejavu-fonts.github.io/License.html
        const DEJAVU_SANS: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

        fonts.font_data.insert(
            "dejavu_sans".to_owned(),
            egui::FontData::from_static(DEJAVU_SANS).into(),
        );

        // Add DejaVu Sans as highest priority for proportional text
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "dejavu_sans".to_owned());

        ctx.set_fonts(fonts);
    }

    /// Increase font size by one step
    fn zoom_in(&mut self) {
        self.current_font_size = (self.current_font_size + FONT_SIZE_STEP).min(MAX_FONT_SIZE);
    }

    /// Decrease font size by one step
    fn zoom_out(&mut self) {
        self.current_font_size = (self.current_font_size - FONT_SIZE_STEP).max(MIN_FONT_SIZE);
    }

    /// Reset font size to base setting
    fn reset_zoom(&mut self) {
        self.current_font_size = self.user_settings.base_font_size;
    }

    /// Check if the current view settings match the active preset
    fn current_view_matches_active_preset(&self) -> bool {
        if let Some(ref preset_name) = self.active_preset {
            if let Some(preset) = self
                .user_settings
                .view_presets
                .iter()
                .find(|p| &p.name == preset_name)
            {
                return self.perspective == preset.perspective
                    && self.perspective_direction == preset.direction
                    && self.filter_types == preset.get_filter_types()
                    && self.filter_features == preset.get_filter_features()
                    && self.filter_prefixes == preset.get_filter_prefixes()
                    && self.child_filter_types == preset.get_child_filter_types()
                    && self.child_filter_features == preset.get_child_filter_features()
                    && self.child_filter_prefixes == preset.get_child_filter_prefixes()
                    && self.children_same_as_root == preset.children_same_as_root;
            }
        }
        false
    }

    /// Check if there's an unsaved view (view differs from active preset or no preset active but has non-default settings)
    fn has_unsaved_view(&self) -> bool {
        if self.active_preset.is_some() {
            // If we have an active preset, check if current view differs
            !self.current_view_matches_active_preset()
        } else {
            // If no active preset, check if we have non-default settings
            self.perspective != Perspective::Flat
                || self.perspective_direction != PerspectiveDirection::TopDown
                || !self.filter_types.is_empty()
                || !self.filter_features.is_empty()
                || !self.filter_prefixes.is_empty()
                || !self.children_same_as_root
                || !self.child_filter_types.is_empty()
                || !self.child_filter_features.is_empty()
                || !self.child_filter_prefixes.is_empty()
        }
    }

    /// Apply a preset to the current view
    fn apply_preset(&mut self, preset: &ViewPreset) {
        self.perspective = preset.perspective.clone();
        self.perspective_direction = preset.direction;
        self.filter_types = preset.get_filter_types();
        self.filter_features = preset.get_filter_features();
        self.filter_prefixes = preset.get_filter_prefixes();
        self.child_filter_types = preset.get_child_filter_types();
        self.child_filter_features = preset.get_child_filter_features();
        self.child_filter_prefixes = preset.get_child_filter_prefixes();
        self.children_same_as_root = preset.children_same_as_root;
        self.active_preset = Some(preset.name.clone());
    }

    /// Save the current view as a new preset
    fn save_current_view_as_preset(&mut self, name: String) {
        let preset = ViewPreset::new(
            name.clone(),
            self.perspective.clone(),
            self.perspective_direction,
            &self.filter_types,
            &self.filter_features,
            &self.filter_prefixes,
            &self.child_filter_types,
            &self.child_filter_features,
            &self.child_filter_prefixes,
            self.children_same_as_root,
        );

        // Check if preset with this name already exists
        if let Some(existing) = self
            .user_settings
            .view_presets
            .iter_mut()
            .find(|p| p.name == name)
        {
            // Update existing preset
            *existing = preset;
        } else {
            // Add new preset
            self.user_settings.view_presets.push(preset);
        }

        self.active_preset = Some(name);

        // Save settings
        if let Err(e) = self.user_settings.save() {
            self.message = Some((format!("Failed to save preset: {}", e), true));
        } else {
            self.message = Some(("View preset saved".to_string(), false));
        }
    }

    /// Delete a preset by name
    fn delete_preset(&mut self, name: &str) {
        self.user_settings.view_presets.retain(|p| p.name != name);

        // If the deleted preset was active, clear it
        if self.active_preset.as_deref() == Some(name) {
            self.active_preset = None;
        }

        // Save settings
        if let Err(e) = self.user_settings.save() {
            self.message = Some((format!("Failed to delete preset: {}", e), true));
        } else {
            self.message = Some(("View preset deleted".to_string(), false));
        }
    }

    /// Reset to default view (user's preferred perspective, TopDown, no filters, clear search)
    fn reset_to_default_view(&mut self) {
        self.perspective = self.user_settings.preferred_perspective.clone();
        self.perspective_direction = PerspectiveDirection::TopDown;
        self.filter_types.clear();
        self.filter_features.clear();
        self.filter_text.clear();
        self.search_scope = SearchScope::all();
        self.active_preset = None;
    }

    /// Open the user guide in the default browser
    fn open_user_guide() {
        // Get the path to the docs directory relative to the executable
        let exe_path = std::env::current_exe().ok();

        let possible_paths: Vec<PathBuf> = if let Some(ref exe) = exe_path {
            vec![
                // Relative to executable (for installed binaries)
                exe.parent().unwrap().join("../docs"),
                exe.parent().unwrap().join("../../docs"),
                // Development paths
                exe.parent().unwrap().join("../../../docs"),
                exe.parent().unwrap().join("../../../../docs"),
                // Current directory
                std::env::current_dir().unwrap_or_default().join("docs"),
                // Project root (when running from project directory)
                PathBuf::from("docs"),
            ]
        } else {
            vec![
                std::env::current_dir().unwrap_or_default().join("docs"),
                PathBuf::from("docs"),
            ]
        };

        let filename = "user-guide.html";

        // Find the first path that exists
        let doc_path = possible_paths
            .iter()
            .map(|p| p.join(filename))
            .find(|p| p.exists());

        if let Some(path) = doc_path {
            if let Ok(canonical) = path.canonicalize() {
                let url = format!("file://{}", canonical.to_string_lossy());

                // Try to open in browser using platform-specific commands
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                }

                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                }

                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", &url])
                        .spawn();
                }
            }
        }
    }

    /// Restart the application, detecting if running via cargo run
    fn restart_application() {
        let exe_path = std::env::current_exe().ok();
        let args: Vec<String> = std::env::args().collect();

        // Detect if we're running from a cargo target directory
        let is_cargo_run = exe_path
            .as_ref()
            .map(|p| {
                let path_str = p.to_string_lossy();
                path_str.contains("/target/debug/") || path_str.contains("/target/release/")
            })
            .unwrap_or(false);

        if is_cargo_run {
            // Running via cargo - use cargo run to potentially trigger recompile
            // Determine the binary name from the exe path
            let bin_name = exe_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("aida-gui");

            // Check if this is a release build
            let is_release = exe_path
                .as_ref()
                .map(|p| p.to_string_lossy().contains("/target/release/"))
                .unwrap_or(false);

            let mut cmd = std::process::Command::new("cargo");
            cmd.arg("run").arg("--bin").arg(bin_name);

            if is_release {
                cmd.arg("--release");
            }

            // Pass through any additional arguments (skip the exe name)
            if args.len() > 1 {
                cmd.arg("--").args(&args[1..]);
            }

            // Spawn the new process detached
            let _ = cmd.spawn();
        } else if let Some(exe) = exe_path {
            // Direct executable - just restart it
            let mut cmd = std::process::Command::new(&exe);

            // Pass through any arguments (skip the exe name)
            if args.len() > 1 {
                cmd.args(&args[1..]);
            }

            // Spawn the new process detached
            let _ = cmd.spawn();
        }
    }

    fn reload(&mut self) {
        if let Ok(store) = self.storage.load() {
            self.store = store;
            self.message = Some(("Reloaded successfully".to_string(), false));
        } else {
            self.message = Some(("Failed to reload".to_string(), true));
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.storage.save(&self.store) {
            self.message = Some((format!("Error saving: {}", e), true));
        } else {
            self.message = Some(("Saved successfully".to_string(), false));
        }
    }

    /// Cycle through all themes including custom ones
    fn cycle_theme(&mut self) {
        // Build list of all available themes: built-ins + custom themes
        let built_in_themes = [
            Theme::Dark,
            Theme::Light,
            Theme::HighContrastDark,
            Theme::SolarizedDark,
            Theme::Nord,
        ];

        // Find current theme index
        let current = &self.user_settings.theme;

        // Check if current is a built-in theme
        let mut current_idx: Option<usize> = None;
        for (i, theme) in built_in_themes.iter().enumerate() {
            if std::mem::discriminant(current) == std::mem::discriminant(theme)
                && !matches!(current, Theme::Custom(_))
            {
                if current == theme {
                    current_idx = Some(i);
                    break;
                }
            }
        }

        // If not found in built-ins, check custom themes
        let custom_themes = &self.user_settings.custom_themes;
        let mut current_custom_idx: Option<usize> = None;
        if current_idx.is_none() {
            if let Theme::Custom(ref current_custom) = current {
                for (i, custom) in custom_themes.iter().enumerate() {
                    if custom.name == current_custom.name {
                        current_custom_idx = Some(i);
                        break;
                    }
                }
            }
        }

        // Determine next theme
        let next_theme = if let Some(idx) = current_idx {
            // Currently on a built-in theme
            if idx + 1 < built_in_themes.len() {
                // Next built-in
                built_in_themes[idx + 1].clone()
            } else if !custom_themes.is_empty() {
                // First custom theme
                Theme::Custom(Box::new(custom_themes[0].clone()))
            } else {
                // Wrap to first built-in
                Theme::Dark
            }
        } else if let Some(idx) = current_custom_idx {
            // Currently on a custom theme
            if idx + 1 < custom_themes.len() {
                // Next custom theme
                Theme::Custom(Box::new(custom_themes[idx + 1].clone()))
            } else {
                // Wrap to first built-in
                Theme::Dark
            }
        } else {
            // Unknown state, reset to Dark
            Theme::Dark
        };

        self.user_settings.theme = next_theme;
    }

    fn clear_form(&mut self) {
        self.form_title.clear();
        self.form_description.clear();
        self.form_status = RequirementStatus::Draft;
        self.form_status_string = String::from("Draft");
        self.form_custom_fields.clear();
        self.form_priority = RequirementPriority::Medium;
        self.form_type = RequirementType::Functional;
        self.form_owner.clear();
        self.form_feature = String::from("Uncategorized");
        self.form_tags.clear();
        self.form_prefix.clear();
        self.show_description_preview = false;

        // If a requirement is selected, pre-populate parent relationship
        self.form_parent_id = self
            .selected_idx
            .and_then(|idx| self.store.requirements.get(idx))
            .map(|req| req.id);

        // Store original values for change detection
        self.store_original_form_values();
    }

    fn load_form_from_requirement(&mut self, idx: usize) {
        if let Some(req) = self.store.requirements.get(idx) {
            self.form_title = req.title.clone();
            self.form_description = req.description.clone();
            self.form_status = req.status.clone();
            self.form_status_string = req.effective_status();
            self.form_custom_fields = req.custom_fields.clone();
            self.form_priority = req.priority.clone();
            self.form_type = req.req_type.clone();
            self.form_owner = req.owner.clone();
            self.form_feature = req.feature.clone();
            let tags_vec: Vec<String> = req.tags.iter().cloned().collect();
            self.form_tags = tags_vec.join(", ");
            self.form_prefix = req.prefix_override.clone().unwrap_or_default();
            self.show_description_preview = false;
        }
        // Store original values for change detection
        self.store_original_form_values();
    }

    /// Store current form values as original values (for change detection on cancel)
    fn store_original_form_values(&mut self) {
        self.original_form_title = self.form_title.clone();
        self.original_form_description = self.form_description.clone();
        self.original_form_status_string = self.form_status_string.clone();
        self.original_form_priority = self.form_priority.clone();
        self.original_form_type = self.form_type.clone();
        self.original_form_owner = self.form_owner.clone();
        self.original_form_feature = self.form_feature.clone();
        self.original_form_tags = self.form_tags.clone();
        self.original_form_prefix = self.form_prefix.clone();
        self.original_form_custom_fields = self.form_custom_fields.clone();
    }

    /// Check if form has unsaved changes compared to original values
    fn form_has_changes(&self) -> bool {
        self.form_title != self.original_form_title
            || self.form_description != self.original_form_description
            || self.form_status_string != self.original_form_status_string
            || self.form_priority != self.original_form_priority
            || self.form_type != self.original_form_type
            || self.form_owner != self.original_form_owner
            || self.form_feature != self.original_form_feature
            || self.form_tags != self.original_form_tags
            || self.form_prefix != self.original_form_prefix
            || self.form_custom_fields != self.original_form_custom_fields
    }

    /// Handle cancel request - either cancel immediately or show confirmation dialog
    fn request_form_cancel(&mut self, is_edit: bool) {
        if self.form_has_changes() {
            self.show_cancel_confirm_dialog = true;
        } else {
            self.cancel_form(is_edit);
        }
    }

    /// Actually cancel the form and return to previous view
    fn cancel_form(&mut self, is_edit: bool) {
        self.clear_form();
        self.show_cancel_confirm_dialog = false;
        self.pending_view_change = Some(if is_edit { View::Detail } else { View::List });
    }

    fn add_requirement(&mut self) {
        let tags: HashSet<String> = self
            .form_tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut req = Requirement::new(self.form_title.clone(), self.form_description.clone());
        // Set status from string (handles both standard and custom statuses)
        req.set_status_from_str(&self.form_status_string);
        req.priority = self.form_priority.clone();
        req.req_type = self.form_type.clone();
        req.owner = self.form_owner.clone();
        req.feature = self.form_feature.clone();
        req.tags = tags;
        // Copy custom field values
        req.custom_fields = self.form_custom_fields.clone();

        // Set prefix override if specified
        let prefix_trimmed = self.form_prefix.trim();
        if !prefix_trimmed.is_empty() {
            if let Err(e) = req.set_prefix_override(prefix_trimmed) {
                self.message = Some((e, true));
                return;
            }
            // Auto-add new prefix to allowed list (if not restricted)
            if !self.store.restrict_prefixes {
                self.store.add_allowed_prefix(prefix_trimmed);
            }
        }

        // Store parent ID before clearing form
        let parent_id = self.form_parent_id;

        // Get prefixes for ID generation
        let feature_prefix = self
            .store
            .get_feature_by_name(&req.feature)
            .map(|f| f.prefix.clone());
        let type_prefix = self.store.get_type_prefix(&req.req_type);

        // Capture the new requirement's ID before adding
        let new_req_id = req.id;

        // Add requirement with auto-assigned ID based on configuration
        self.store
            .add_requirement_with_id(req, feature_prefix.as_deref(), type_prefix.as_deref());

        // Create parent relationship if specified
        if let Some(parent_id) = parent_id {
            // New requirement (child) stores Parent relationship pointing to parent
            let _ = self.store.add_relationship(
                &new_req_id,
                RelationshipType::Parent,
                &parent_id,
                true, // bidirectional
            );
        }

        self.save();
        self.form_parent_id = None; // Clear parent after adding
        self.clear_form();

        // Find the index of the newly added requirement and select it
        if let Some(idx) = self
            .store
            .requirements
            .iter()
            .position(|r| r.id == new_req_id)
        {
            self.selected_idx = Some(idx);
            self.scroll_to_requirement = Some(new_req_id);
            self.current_view = View::Detail; // Show the new requirement's details
        } else {
            self.current_view = View::List;
        }

        self.message = Some(("Requirement added successfully".to_string(), false));
    }

    fn update_requirement(&mut self, idx: usize) {
        // Gather data we need before mutable borrows
        let (req_uuid, old_prefix_override, old_feature, old_req_type) = {
            if let Some(req) = self.store.requirements.get(idx) {
                (
                    req.id,
                    req.prefix_override.clone(),
                    req.feature.clone(),
                    req.req_type.clone(),
                )
            } else {
                return;
            }
        };

        // Determine new prefix
        let new_prefix = if self.form_prefix.trim().is_empty() {
            None
        } else {
            // Auto-add new prefix to allowed list (if not restricted)
            if !self.store.restrict_prefixes {
                self.store.add_allowed_prefix(self.form_prefix.trim());
            }
            Requirement::validate_prefix(&self.form_prefix)
        };

        // Check if we need to regenerate the spec_id
        let prefix_changed = new_prefix != old_prefix_override;
        let feature_changed = self.form_feature != old_feature;
        let type_changed = self.form_type != old_req_type;
        let needs_new_spec_id =
            prefix_changed || (new_prefix.is_none() && (feature_changed || type_changed));

        // Generate new spec_id if needed
        let new_spec_id_result = if needs_new_spec_id {
            let feature_prefix = self
                .store
                .get_feature_by_name(&self.form_feature)
                .map(|f| f.prefix.clone());
            let type_prefix = self.store.get_type_prefix(&self.form_type);

            Some(self.store.regenerate_spec_id_for_prefix_change(
                &req_uuid,
                new_prefix.as_deref(),
                feature_prefix.as_deref(),
                type_prefix.as_deref(),
            ))
        } else {
            None
        };

        // Check for ID conflict
        if let Some(Err(ref error_msg)) = new_spec_id_result {
            self.message = Some((error_msg.clone(), true));
            return;
        }

        // Now perform the actual updates
        if let Some(req) = self.store.requirements.get_mut(idx) {
            let mut changes: Vec<FieldChange> = Vec::new();

            // Track title change
            if self.form_title != req.title {
                changes.push(Requirement::field_change(
                    "title",
                    req.title.clone(),
                    self.form_title.clone(),
                ));
                req.title = self.form_title.clone();
            }

            // Track description change
            if self.form_description != req.description {
                changes.push(Requirement::field_change(
                    "description",
                    req.description.clone(),
                    self.form_description.clone(),
                ));
                req.description = self.form_description.clone();
            }

            // Track status change (use effective_status for comparison)
            let old_status = req.effective_status();
            if self.form_status_string != old_status {
                changes.push(Requirement::field_change(
                    "status",
                    old_status,
                    self.form_status_string.clone(),
                ));
                req.set_status_from_str(&self.form_status_string);
            }

            // Track custom fields changes
            for (key, new_value) in &self.form_custom_fields {
                let old_value = req.custom_fields.get(key).cloned().unwrap_or_default();
                if *new_value != old_value {
                    changes.push(Requirement::field_change(key, old_value, new_value.clone()));
                }
            }
            req.custom_fields = self.form_custom_fields.clone();

            // Track priority change
            if self.form_priority != req.priority {
                changes.push(Requirement::field_change(
                    "priority",
                    format!("{:?}", req.priority),
                    format!("{:?}", self.form_priority),
                ));
                req.priority = self.form_priority.clone();
            }

            // Track type change
            if self.form_type != req.req_type {
                changes.push(Requirement::field_change(
                    "type",
                    format!("{:?}", req.req_type),
                    format!("{:?}", self.form_type),
                ));
                req.req_type = self.form_type.clone();
            }

            // Track owner change
            if self.form_owner != req.owner {
                changes.push(Requirement::field_change(
                    "owner",
                    req.owner.clone(),
                    self.form_owner.clone(),
                ));
                req.owner = self.form_owner.clone();
            }

            // Track feature change
            if self.form_feature != req.feature {
                changes.push(Requirement::field_change(
                    "feature",
                    req.feature.clone(),
                    self.form_feature.clone(),
                ));
                req.feature = self.form_feature.clone();
            }

            // Track tags change
            let new_tags: HashSet<String> = self
                .form_tags
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if new_tags != req.tags {
                let old_tags_vec: Vec<String> = req.tags.iter().cloned().collect();
                let new_tags_vec: Vec<String> = new_tags.iter().cloned().collect();
                changes.push(Requirement::field_change(
                    "tags",
                    old_tags_vec.join(", "),
                    new_tags_vec.join(", "),
                ));
                req.tags = new_tags;
            }

            // Track prefix override change and update spec_id
            if prefix_changed {
                let old_prefix_str = req.prefix_override.clone().unwrap_or_default();
                let new_prefix_str = new_prefix.clone().unwrap_or_default();
                changes.push(Requirement::field_change(
                    "prefix_override",
                    old_prefix_str,
                    new_prefix_str,
                ));
                req.prefix_override = new_prefix;
            }

            // Update spec_id if it changed
            if let Some(Ok(ref new_id)) = new_spec_id_result {
                let old_id = req.spec_id.clone().unwrap_or_default();
                if old_id != *new_id {
                    changes.push(Requirement::field_change("spec_id", old_id, new_id.clone()));
                    req.spec_id = Some(new_id.clone());
                }
            }

            // Record changes with author from user settings
            req.record_change(self.user_settings.display_name(), changes);

            self.save();
            self.clear_form();
            self.current_view = View::Detail;
            self.message = Some(("Requirement updated successfully".to_string(), false));
        }
    }

    fn delete_requirement(&mut self, idx: usize) {
        if idx < self.store.requirements.len() {
            self.store.requirements.remove(idx);
            self.save();
            self.selected_idx = None;
            self.current_view = View::List;
            self.message = Some(("Requirement deleted successfully".to_string(), false));
        }
    }

    fn toggle_archive(&mut self, idx: usize) {
        let (new_archived, author) = {
            if let Some(req) = self.store.requirements.get(idx) {
                (!req.archived, self.user_settings.display_name())
            } else {
                return;
            }
        };

        if let Some(req) = self.store.requirements.get_mut(idx) {
            let was_archived = req.archived;
            req.archived = new_archived;

            // Record change in history
            let change = Requirement::field_change(
                "archived",
                was_archived.to_string(),
                new_archived.to_string(),
            );
            req.record_change(author, vec![change]);
        }

        self.save();
        let action = if new_archived {
            "archived"
        } else {
            "unarchived"
        };
        self.message = Some((format!("Requirement {}", action), false));
    }

    fn show_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // Menu dropdown
                ui.menu_button("☰ Menu", |ui| {
                    if ui.button("🔄 Reload").clicked() {
                        self.reload();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("📁 Switch Project...").clicked() {
                        self.load_available_projects();
                        self.show_switch_project_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("➕ New Project...").clicked() {
                        self.clear_new_project_form();
                        self.show_new_project_dialog = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("🔄 Restart").clicked() {
                        Self::restart_application();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("🚪 Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                if ui.button("➕ Add").clicked() {
                    self.clear_form();
                    self.pending_view_change = Some(View::Add);
                }

                ui.separator();
                ui.label(format!("Requirements: {}", self.store.requirements.len()));

                // Show message
                if let Some((msg, is_error)) = &self.message {
                    ui.separator();
                    let color = if *is_error {
                        egui::Color32::RED
                    } else {
                        egui::Color32::GREEN
                    };
                    ui.colored_label(color, msg);
                }

                // Settings and help buttons (right-aligned)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⚙").on_hover_text("Settings").clicked() {
                        // Load current user settings into form
                        self.settings_form_name = self.user_settings.name.clone();
                        self.settings_form_email = self.user_settings.email.clone();
                        self.settings_form_handle = self.user_settings.handle.clone();
                        self.settings_form_font_size = self.user_settings.base_font_size;
                        self.settings_form_ui_heading_level = self.user_settings.ui_heading_level;
                        self.settings_form_perspective =
                            self.user_settings.preferred_perspective.clone();
                        self.settings_form_theme = self.user_settings.theme.clone();
                        self.settings_form_keybindings = self.user_settings.keybindings.clone();
                        self.settings_form_show_status_icons =
                            self.user_settings.show_status_icons;
                        self.settings_form_status_icons =
                            self.user_settings.status_icons.clone();
                        self.settings_form_priority_icons =
                            self.user_settings.priority_icons.clone();
                        self.capturing_key_for = None;

                        // Store original appearance settings for Cancel reversion
                        self.original_appearance_theme = self.user_settings.theme.clone();
                        self.original_appearance_font_size = self.user_settings.base_font_size;
                        self.original_appearance_ui_heading_level = self.user_settings.ui_heading_level;
                        self.original_appearance_show_status_icons = self.user_settings.show_status_icons;
                        self.original_appearance_status_icons = self.user_settings.status_icons.clone();
                        self.original_appearance_priority_icons = self.user_settings.priority_icons.clone();

                        // Load current project settings into form
                        self.settings_form_id_format = self.store.id_config.format.clone();
                        self.settings_form_numbering = self.store.id_config.numbering.clone();
                        self.settings_form_digits = self.store.id_config.digits;
                        self.show_settings_dialog = true;
                    }
                    if ui
                        .button("?")
                        .on_hover_text("Help - Open User Guide")
                        .clicked()
                    {
                        Self::open_user_guide();
                    }
                    // Show current zoom level
                    ui.label(format!("{}pt", self.current_font_size as i32));
                });
            });
        });
    }

    /// Load available projects from registry
    fn load_available_projects(&mut self) {
        self.available_projects.clear();
        if let Ok(registry_path) = aida_core::get_registry_path() {
            if registry_path.exists() {
                if let Ok(registry) = aida_core::Registry::load(&registry_path) {
                    for (name, project) in &registry.projects {
                        // Only include projects whose files exist
                        let path = std::path::Path::new(&project.path);
                        if path.exists() || path.is_relative() {
                            self.available_projects.push((
                                name.clone(),
                                project.path.clone(),
                                project.description.clone(),
                            ));
                        }
                    }
                    // Sort by name
                    self.available_projects.sort_by(|a, b| a.0.cmp(&b.0));
                }
            }
        }
    }

    /// Clear the new project form
    fn clear_new_project_form(&mut self) {
        self.new_project_dir = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        self.new_project_name = String::new();
        self.new_project_title = String::new();
        self.new_project_description = String::new();
        self.new_project_template = "current".to_string();
        self.new_project_include_users = false;
    }

    /// Switch to a different project
    fn switch_project(&mut self, path: &str) {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            self.storage = Storage::new(path.clone());
            if let Ok(store) = self.storage.load() {
                self.store = store;
                self.selected_idx = None;
                self.current_view = View::List;
                self.message = Some((format!("Switched to project: {}", path.display()), false));
            } else {
                self.message = Some((format!("Failed to load project: {}", path.display()), true));
            }
        } else {
            self.message = Some((format!("Project file not found: {}", path.display()), true));
        }
    }

    /// Create a new project from template
    fn create_new_project(&mut self) -> Result<(), String> {
        use std::fs;
        use std::path::PathBuf;

        let dir = PathBuf::from(&self.new_project_dir);
        if !dir.exists() {
            return Err(format!("Directory does not exist: {}", dir.display()));
        }

        let project_file = dir.join("requirements.yaml");
        if project_file.exists() {
            return Err(format!(
                "A requirements.yaml already exists in: {}",
                dir.display()
            ));
        }

        // Create new store from template
        let mut new_store = if self.new_project_template == "current" {
            // Copy from current project
            let mut store = self.store.clone();
            store.requirements.clear();
            store.next_spec_number = 1;
            store.prefix_counters.clear();
            if !self.new_project_include_users {
                // Keep only current user
                let current_user = store
                    .users
                    .iter()
                    .find(|u| {
                        u.handle == self.user_settings.handle || u.email == self.user_settings.email
                    })
                    .cloned();
                store.users.clear();
                if let Some(user) = current_user {
                    store.users.push(user);
                }
            }
            store
        } else {
            // Load from template file
            let templates_dir = aida_core::get_templates_dir().map_err(|e| e.to_string())?;
            let template_file = templates_dir.join(format!("{}.yaml", self.new_project_template));
            if template_file.exists() {
                let content = fs::read_to_string(&template_file)
                    .map_err(|e| format!("Failed to read template: {}", e))?;
                serde_yaml::from_str::<RequirementsStore>(&content)
                    .map_err(|e| format!("Failed to parse template: {}", e))?
            } else {
                return Err(format!("Template not found: {}", self.new_project_template));
            }
        };

        // Set project title if provided
        if !self.new_project_title.is_empty() {
            new_store.title = self.new_project_title.clone();
        }

        // Save the new project file
        let content =
            serde_yaml::to_string(&new_store).map_err(|e| format!("Failed to serialize: {}", e))?;
        fs::write(&project_file, content).map_err(|e| format!("Failed to write file: {}", e))?;

        // Register in registry
        if let Ok(registry_path) = aida_core::get_registry_path() {
            let mut registry = if registry_path.exists() {
                aida_core::Registry::load(&registry_path).unwrap_or_else(|_| aida_core::Registry {
                    projects: std::collections::HashMap::new(),
                    default_project: None,
                })
            } else {
                aida_core::Registry {
                    projects: std::collections::HashMap::new(),
                    default_project: None,
                }
            };

            registry.register_project(
                self.new_project_name.clone(),
                project_file.to_string_lossy().to_string(),
                self.new_project_description.clone(),
            );

            if let Err(e) = registry.save(&registry_path) {
                return Err(format!("Failed to update registry: {}", e));
            }
        }

        // Switch to the new project
        self.switch_project(&project_file.to_string_lossy());

        Ok(())
    }

    /// Show the switch project dialog
    fn show_switch_project_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_switch_project_dialog {
            return;
        }

        let mut close_dialog = false;
        let mut switch_to: Option<String> = None;

        egui::Window::new("📁 Switch Project")
            .collapsible(false)
            .resizable(true)
            .min_width(400.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.heading("Available Projects");
                ui.add_space(10.0);

                if self.available_projects.is_empty() {
                    ui.label("No projects found in registry.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (name, path, description) in &self.available_projects {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.strong(name);
                                        if ui.small_button("Open").clicked() {
                                            switch_to = Some(path.clone());
                                        }
                                    });
                                    ui.label(
                                        egui::RichText::new(path)
                                            .small()
                                            .color(egui::Color32::GRAY),
                                    );
                                    if !description.is_empty() {
                                        ui.label(description);
                                    }
                                });
                            }
                        });
                }

                ui.add_space(10.0);
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            });

        if let Some(path) = switch_to {
            self.switch_project(&path);
            close_dialog = true;
        }

        if close_dialog {
            self.show_switch_project_dialog = false;
        }
    }

    /// Show the new project dialog
    fn show_new_project_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_new_project_dialog {
            return;
        }

        let mut close_dialog = false;
        let mut create_project = false;

        egui::Window::new("➕ New Project")
            .collapsible(false)
            .resizable(true)
            .min_width(450.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.heading("Create New Project");
                ui.add_space(10.0);

                egui::Grid::new("new_project_grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Directory:");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_project_dir)
                                    .desired_width(300.0),
                            );
                        });
                        ui.end_row();

                        ui.label("Project Name:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_project_name)
                                .hint_text("my-project")
                                .desired_width(200.0),
                        );
                        ui.end_row();

                        ui.label("Title:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_project_title)
                                .hint_text("My Project Requirements")
                                .desired_width(300.0),
                        );
                        ui.end_row();

                        ui.label("Description:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_project_description)
                                .hint_text("Brief description")
                                .desired_width(300.0),
                        );
                        ui.end_row();

                        ui.label("Template:");
                        egui::ComboBox::from_id_salt("template_combo")
                            .selected_text(&self.new_project_template)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.new_project_template,
                                    "current".to_string(),
                                    "Current Project",
                                );
                                // TODO: Add templates from ~/.config/aida/templates/
                            });
                        ui.end_row();

                        ui.label("Include Users:");
                        ui.checkbox(
                            &mut self.new_project_include_users,
                            "Copy all users from template",
                        );
                        ui.end_row();
                    });

                ui.add_space(15.0);
                ui.separator();

                ui.horizontal(|ui| {
                    let can_create =
                        !self.new_project_dir.is_empty() && !self.new_project_name.is_empty();

                    if ui
                        .add_enabled(can_create, egui::Button::new("Create"))
                        .clicked()
                    {
                        create_project = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            });

        if create_project {
            match self.create_new_project() {
                Ok(()) => {
                    self.message = Some(("Project created successfully".to_string(), false));
                    close_dialog = true;
                }
                Err(e) => {
                    self.message = Some((e, true));
                }
            }
        }

        if close_dialog {
            self.show_new_project_dialog = false;
        }
    }

    fn show_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_settings_dialog {
            return;
        }

        egui::Window::new("⚙ Settings")
            .collapsible(false)
            .resizable(true)
            .min_width(400.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // Tabs
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::User, "👤 User");
                    ui.selectable_value(
                        &mut self.settings_tab,
                        SettingsTab::Appearance,
                        "🎨 Appearance",
                    );
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Keybindings, "⌨ Keys");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::IDs, "🔢 IDs");
                    ui.selectable_value(
                        &mut self.settings_tab,
                        SettingsTab::Relationships,
                        "🔗 Relations",
                    );
                    ui.selectable_value(
                        &mut self.settings_tab,
                        SettingsTab::Reactions,
                        "😊 Reactions",
                    );
                    ui.selectable_value(
                        &mut self.settings_tab,
                        SettingsTab::TypeDefinitions,
                        "📝 Types",
                    );
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Users, "👥 Users");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Database, "🗄 Db");
                });

                ui.separator();
                ui.add_space(10.0);

                // Tab content
                match self.settings_tab {
                    SettingsTab::User => {
                        self.show_settings_user_tab(ui);
                    }
                    SettingsTab::Appearance => {
                        self.show_settings_appearance_tab(ui);
                    }
                    SettingsTab::Keybindings => {
                        self.show_settings_keybindings_tab(ui, ctx);
                    }
                    SettingsTab::IDs => {
                        self.show_settings_ids_tab(ui);
                    }
                    SettingsTab::Relationships => {
                        self.show_settings_relationships_tab(ui);
                    }
                    SettingsTab::Reactions => {
                        self.show_settings_reactions_tab(ui);
                    }
                    SettingsTab::TypeDefinitions => {
                        self.show_settings_type_definitions_tab(ui);
                    }
                    SettingsTab::Users => {
                        self.show_settings_users_tab(ui);
                    }
                    SettingsTab::Database => {
                        self.show_settings_database_tab(ui);
                    }
                }

                ui.add_space(15.0);
                ui.separator();
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        // Update user settings from form
                        self.user_settings.name = self.settings_form_name.clone();
                        self.user_settings.email = self.settings_form_email.clone();
                        self.user_settings.handle = self.settings_form_handle.clone();
                        self.user_settings.base_font_size = self.settings_form_font_size;
                        self.user_settings.ui_heading_level = self.settings_form_ui_heading_level;
                        self.user_settings.preferred_perspective =
                            self.settings_form_perspective.clone();
                        self.user_settings.theme = self.settings_form_theme.clone();
                        self.user_settings.keybindings = self.settings_form_keybindings.clone();
                        self.user_settings.show_status_icons =
                            self.settings_form_show_status_icons;
                        self.user_settings.status_icons =
                            self.settings_form_status_icons.clone();
                        self.user_settings.priority_icons =
                            self.settings_form_priority_icons.clone();

                        // Update project settings (stored in requirements file)
                        self.store.id_config.format = self.settings_form_id_format.clone();
                        self.store.id_config.numbering = self.settings_form_numbering.clone();
                        self.store.id_config.digits = self.settings_form_digits;

                        // Apply the new base font size as current
                        self.current_font_size = self.settings_form_font_size;

                        // Apply the new preferred perspective
                        self.perspective = self.settings_form_perspective.clone();

                        // Theme will be applied on next frame via update()

                        // Save user settings to file
                        let mut save_success = true;
                        match self.user_settings.save() {
                            Ok(()) => {}
                            Err(e) => {
                                self.message =
                                    Some((format!("Failed to save user settings: {}", e), true));
                                save_success = false;
                            }
                        }

                        // Save project settings (requirements store) to file
                        if save_success {
                            match self.storage.save(&self.store) {
                                Ok(()) => {
                                    self.message =
                                        Some(("Settings saved successfully".to_string(), false));
                                }
                                Err(e) => {
                                    self.message = Some((
                                        format!("Failed to save project settings: {}", e),
                                        true,
                                    ));
                                }
                            }
                        }
                        self.show_settings_dialog = false;
                    }

                    if ui.button("❌ Cancel").clicked() {
                        // Revert appearance settings to original values (live preview cleanup)
                        self.user_settings.theme = self.original_appearance_theme.clone();
                        self.user_settings.base_font_size = self.original_appearance_font_size;
                        self.current_font_size = self.original_appearance_font_size;
                        self.user_settings.ui_heading_level = self.original_appearance_ui_heading_level;
                        self.user_settings.show_status_icons = self.original_appearance_show_status_icons;
                        self.user_settings.status_icons = self.original_appearance_status_icons.clone();
                        self.user_settings.priority_icons = self.original_appearance_priority_icons.clone();
                        self.show_settings_dialog = false;
                    }
                });
            });
    }

    fn show_settings_user_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("User Profile");
        ui.add_space(10.0);

        egui::Grid::new("settings_user_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                ui.label("Name:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings_form_name)
                        .hint_text("Your full name"),
                );
                ui.end_row();

                ui.label("Email:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings_form_email)
                        .hint_text("your.email@example.com"),
                );
                ui.end_row();

                ui.label("Handle (@):");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings_form_handle)
                        .hint_text("nickname for @mentions"),
                );
                ui.end_row();
            });
    }

    fn show_settings_appearance_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Display Settings");
        ui.add_space(10.0);

        egui::Grid::new("settings_appearance_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                ui.label("Theme:");
                ui.horizontal(|ui| {
                    // Track if theme changed to apply it immediately
                    let old_theme = self.settings_form_theme.clone();

                    // Collect saved custom themes first to avoid borrow issues
                    let saved_customs: Vec<(String, CustomTheme)> = self
                        .user_settings
                        .custom_themes
                        .iter()
                        .map(|ct| (ct.name.clone(), ct.clone()))
                        .collect();

                    egui::ComboBox::from_id_salt("settings_theme_combo")
                        .selected_text(self.settings_form_theme.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.settings_form_theme,
                                Theme::Dark,
                                Theme::Dark.label(),
                            );
                            ui.selectable_value(
                                &mut self.settings_form_theme,
                                Theme::Light,
                                Theme::Light.label(),
                            );
                            ui.selectable_value(
                                &mut self.settings_form_theme,
                                Theme::HighContrastDark,
                                Theme::HighContrastDark.label(),
                            );
                            ui.selectable_value(
                                &mut self.settings_form_theme,
                                Theme::SolarizedDark,
                                Theme::SolarizedDark.label(),
                            );
                            ui.selectable_value(
                                &mut self.settings_form_theme,
                                Theme::Nord,
                                Theme::Nord.label(),
                            );

                            // Show saved custom themes
                            if !saved_customs.is_empty() {
                                ui.separator();
                                for (name, custom) in saved_customs {
                                    ui.selectable_value(
                                        &mut self.settings_form_theme,
                                        Theme::Custom(Box::new(custom)),
                                        format!("Custom: {}", name),
                                    );
                                }
                            }
                        });

                    // Apply theme change immediately for live preview
                    if self.settings_form_theme != old_theme {
                        self.user_settings.theme = self.settings_form_theme.clone();
                    }

                    if ui.button("🎨 Edit Theme...").clicked() {
                        // Store original theme for Cancel
                        self.theme_editor_original_theme = self.settings_form_theme.clone();

                        // Initialize the theme editor with current theme or create new
                        self.theme_editor_theme = if let Theme::Custom(ref custom) = self.settings_form_theme {
                            (**custom).clone()
                        } else {
                            // Create a new custom theme based on current theme
                            let base = match self.settings_form_theme {
                                Theme::Light => BaseTheme::Light,
                                _ => BaseTheme::Dark,
                            };
                            CustomTheme::from_base(base, "My Theme".to_string())
                        };
                        self.theme_editor_category = ThemeEditorCategory::default();
                        self.show_theme_editor = true;
                    }
                });
                ui.end_row();

                ui.label("Base Font Size:");
                ui.horizontal(|ui| {
                    let old_font_size = self.settings_form_font_size;
                    ui.add(
                        egui::Slider::new(
                            &mut self.settings_form_font_size,
                            MIN_FONT_SIZE..=MAX_FONT_SIZE,
                        )
                        .suffix("pt")
                        .step_by(1.0),
                    );
                    if ui.button("Reset").clicked() {
                        self.settings_form_font_size = DEFAULT_FONT_SIZE;
                    }
                    // Apply font size change immediately for live preview
                    if self.settings_form_font_size != old_font_size {
                        self.user_settings.base_font_size = self.settings_form_font_size;
                        self.current_font_size = self.settings_form_font_size;
                    }
                });
                ui.end_row();

                ui.label("UI Title Size:");
                ui.horizontal(|ui| {
                    let old_heading_level = self.settings_form_ui_heading_level;
                    egui::ComboBox::from_id_salt("settings_heading_level_combo")
                        .selected_text(format!("H{}", self.settings_form_ui_heading_level))
                        .show_ui(ui, |ui| {
                            for level in 1..=6_u8 {
                                ui.selectable_value(
                                    &mut self.settings_form_ui_heading_level,
                                    level,
                                    format!("H{}", level),
                                );
                            }
                        });
                    if ui.button("Reset").clicked() {
                        self.settings_form_ui_heading_level = default_ui_heading_level();
                    }
                    // Apply heading level change immediately for live preview
                    if self.settings_form_ui_heading_level != old_heading_level {
                        self.user_settings.ui_heading_level = self.settings_form_ui_heading_level;
                    }
                });
                ui.end_row();

                ui.label("Default View:");
                egui::ComboBox::from_id_salt("settings_perspective_combo")
                    .selected_text(self.settings_form_perspective.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.settings_form_perspective,
                            Perspective::Flat,
                            Perspective::Flat.label(),
                        );
                        ui.selectable_value(
                            &mut self.settings_form_perspective,
                            Perspective::ParentChild,
                            Perspective::ParentChild.label(),
                        );
                        ui.selectable_value(
                            &mut self.settings_form_perspective,
                            Perspective::Verification,
                            Perspective::Verification.label(),
                        );
                        ui.selectable_value(
                            &mut self.settings_form_perspective,
                            Perspective::References,
                            Perspective::References.label(),
                        );
                    });
                ui.end_row();

                ui.label("Status Icons:");
                ui.horizontal(|ui| {
                    let old_show_icons = self.settings_form_show_status_icons;
                    ui.checkbox(
                        &mut self.settings_form_show_status_icons,
                        "Show status icons",
                    );
                    // Apply status icons toggle immediately for live preview
                    if self.settings_form_show_status_icons != old_show_icons {
                        self.user_settings.show_status_icons = self.settings_form_show_status_icons;
                    }
                    if ui.button("Edit Icons...").clicked() {
                        self.show_icon_editor = true;
                    }
                });
                ui.end_row();
            });

        ui.add_space(5.0);
        ui.label("Tip: Use Ctrl+MouseWheel or Ctrl+Plus/Minus to zoom");
    }

    /// Show the icon editor dialog
    fn show_icon_editor_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_icon_editor {
            return;
        }

        egui::Window::new("Status & Priority Icons")
            .collapsible(false)
            .resizable(true)
            .default_width(650.0)
            .default_height(700.0)
            .show(ctx, |ui| {
                ui.heading("Configure Status Icons");
                ui.add_space(5.0);
                ui.label("Define icons for different status keywords. Icons are matched using 'contains' matching.");
                ui.add_space(10.0);

                // Status icons section
                ui.group(|ui| {
                    ui.heading("Status Icons");
                    ui.add_space(5.0);

                    // Get sorted list of status keywords
                    let mut status_keys: Vec<String> =
                        self.settings_form_status_icons.icons.keys().cloned().collect();
                    status_keys.sort();

                    egui::ScrollArea::vertical()
                        .id_salt("status_icons_scroll")
                        .max_height(250.0)
                        .show(ui, |ui| {
                            let mut to_remove: Option<String> = None;
                            egui::Grid::new("status_icons_grid")
                                .num_columns(3)
                                .spacing([10.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Keyword").strong());
                                    ui.label(egui::RichText::new("Icon").strong());
                                    ui.label("");
                                    ui.end_row();

                                    for key in &status_keys {
                                        ui.label(key);
                                        if let Some(icon) = self.settings_form_status_icons.icons.get_mut(key) {
                                            let response = ui.add(
                                                egui::TextEdit::singleline(icon).desired_width(60.0),
                                            );
                                            if response.clicked() {
                                                self.symbol_picker_target = Some(format!("status:{}", key));
                                                self.show_symbol_picker = true;
                                            }
                                        }
                                        if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                                            to_remove = Some(key.clone());
                                        }
                                        ui.end_row();
                                    }
                                });
                            if let Some(key) = to_remove {
                                self.settings_form_status_icons.icons.remove(&key);
                            }
                        });

                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.label("Default:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings_form_status_icons.default_icon)
                                .desired_width(60.0),
                        );
                    });

                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.label("Add new:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.icon_editor_new_keyword)
                                .desired_width(100.0)
                                .hint_text("keyword"),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.icon_editor_new_icon)
                                .desired_width(60.0)
                                .hint_text("icon"),
                        );
                        if ui.button("Add Status").clicked()
                            && !self.icon_editor_new_keyword.is_empty()
                        {
                            let keyword = self.icon_editor_new_keyword.to_lowercase();
                            let icon = if self.icon_editor_new_icon.is_empty() {
                                "[*]".to_string()
                            } else {
                                self.icon_editor_new_icon.clone()
                            };
                            self.settings_form_status_icons.icons.insert(keyword, icon);
                            self.icon_editor_new_keyword.clear();
                            self.icon_editor_new_icon.clear();
                        }
                    });
                });

                ui.add_space(15.0);

                // Priority icons section
                ui.group(|ui| {
                    ui.heading("Priority Icons");
                    ui.add_space(5.0);

                    let mut priority_keys: Vec<String> =
                        self.settings_form_priority_icons.icons.keys().cloned().collect();
                    priority_keys.sort();

                    egui::ScrollArea::vertical()
                        .id_salt("priority_icons_scroll")
                        .max_height(200.0)
                        .show(ui, |ui| {
                            let mut to_remove: Option<String> = None;
                            egui::Grid::new("priority_icons_grid")
                                .num_columns(3)
                                .spacing([10.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Priority").strong());
                                    ui.label(egui::RichText::new("Icon").strong());
                                    ui.label("");
                                    ui.end_row();

                                    for key in &priority_keys {
                                        ui.label(key);
                                        if let Some(icon) = self.settings_form_priority_icons.icons.get_mut(key) {
                                            ui.add(
                                                egui::TextEdit::singleline(icon).desired_width(60.0),
                                            );
                                        }
                                        if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                                            to_remove = Some(key.clone());
                                        }
                                        ui.end_row();
                                    }
                                });
                            if let Some(key) = to_remove {
                                self.settings_form_priority_icons.icons.remove(&key);
                            }
                        });

                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.label("Default:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings_form_priority_icons.default_icon)
                                .desired_width(60.0),
                        );
                    });
                });

                ui.add_space(15.0);

                // Symbol picker section
                ui.group(|ui| {
                    ui.heading("Quick Symbols");
                    ui.label("Click to copy, then paste into an icon field:");
                    ui.add_space(5.0);

                    // Common symbols organized by category
                    let symbols = vec![
                        ("Checkmarks", vec!["✓", "✗", "✔", "✘", "☑", "☐", "☒"]),
                        ("Shapes", vec!["●", "○", "◆", "◇", "■", "□", "▸", "▹", "◐", "◑"]),
                        ("Brackets", vec!["[x]", "[ ]", "[-]", "[+]", "[~]", "[?]", "[!]", "[.]", "[*]"]),
                        ("Arrows", vec!["→", "←", "↑", "↓", "⇒", "⇐", "↔", "⟹"]),
                        ("Stars", vec!["★", "☆", "✦", "✧", "⭐", "✪"]),
                        ("Misc", vec!["•", "·", "‣", "⁃", "※", "†", "‡", "§"]),
                    ];

                    for (category, syms) in symbols {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}:", category));
                            for sym in syms {
                                if ui.small_button(sym).on_hover_text("Click to copy").clicked() {
                                    ui.output_mut(|o| o.copied_text = sym.to_string());
                                }
                            }
                        });
                    }
                });

                ui.add_space(15.0);

                // Apply icon changes for live preview
                self.user_settings.status_icons = self.settings_form_status_icons.clone();
                self.user_settings.priority_icons = self.settings_form_priority_icons.clone();

                // Buttons
                ui.horizontal(|ui| {
                    if ui.button("Reset to Defaults").clicked() {
                        self.settings_form_status_icons = StatusIconConfig::default();
                        self.settings_form_priority_icons = PriorityIconConfig::default();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            self.show_icon_editor = false;
                        }
                    });
                });
            });
    }

    fn show_settings_keybindings_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Keyboard Shortcuts");
        ui.add_space(10.0);

        // If we're capturing a key, show instructions
        if let Some(action) = self.capturing_key_for {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("Press a key for '{}' (Escape to cancel)", action.label()),
            );
            ui.add_space(5.0);

            // Get the current context for this action (to preserve it)
            let current_context = self
                .settings_form_keybindings
                .bindings
                .get(&action)
                .map(|b| b.context)
                .unwrap_or(action.default_context());

            // Check for key press
            let captured = ctx.input(|i| {
                // Cancel with Escape
                if i.key_pressed(egui::Key::Escape) {
                    return Some(None);
                }
                // Check for any key press
                for key in [
                    egui::Key::ArrowUp,
                    egui::Key::ArrowDown,
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowRight,
                    egui::Key::Enter,
                    egui::Key::Space,
                    egui::Key::Tab,
                    egui::Key::Backspace,
                    egui::Key::Delete,
                    egui::Key::Home,
                    egui::Key::End,
                    egui::Key::PageUp,
                    egui::Key::PageDown,
                    egui::Key::Plus,
                    egui::Key::Minus,
                    egui::Key::Equals,
                    egui::Key::Num0,
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                    egui::Key::Num6,
                    egui::Key::Num7,
                    egui::Key::Num8,
                    egui::Key::Num9,
                    egui::Key::A,
                    egui::Key::B,
                    egui::Key::C,
                    egui::Key::D,
                    egui::Key::E,
                    egui::Key::F,
                    egui::Key::G,
                    egui::Key::H,
                    egui::Key::I,
                    egui::Key::J,
                    egui::Key::K,
                    egui::Key::L,
                    egui::Key::M,
                    egui::Key::N,
                    egui::Key::O,
                    egui::Key::P,
                    egui::Key::Q,
                    egui::Key::R,
                    egui::Key::S,
                    egui::Key::T,
                    egui::Key::U,
                    egui::Key::V,
                    egui::Key::W,
                    egui::Key::X,
                    egui::Key::Y,
                    egui::Key::Z,
                    egui::Key::F1,
                    egui::Key::F2,
                    egui::Key::F3,
                    egui::Key::F4,
                    egui::Key::F5,
                    egui::Key::F6,
                    egui::Key::F7,
                    egui::Key::F8,
                    egui::Key::F9,
                    egui::Key::F10,
                    egui::Key::F11,
                    egui::Key::F12,
                ] {
                    if i.key_pressed(key) {
                        let binding = KeyBinding {
                            key_name: key_to_string(key).to_string(),
                            ctrl: i.modifiers.ctrl,
                            shift: i.modifiers.shift,
                            alt: i.modifiers.alt,
                            context: current_context, // Preserve the context
                        };
                        return Some(Some(binding));
                    }
                }
                None
            });

            if let Some(result) = captured {
                if let Some(binding) = result {
                    // Set the new binding
                    self.settings_form_keybindings
                        .bindings
                        .insert(action, binding);
                }
                self.capturing_key_for = None;
            }
        }

        // Keybindings table
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("keybindings_grid")
                .num_columns(4)
                .spacing([15.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    // Header
                    ui.strong("Action");
                    ui.strong("Key");
                    ui.strong("Context");
                    ui.strong("");
                    ui.end_row();

                    // Clone the actions to avoid borrow issues
                    let actions: Vec<KeyAction> = KeyAction::all().to_vec();

                    for action in actions {
                        ui.label(action.label());

                        let binding_display = self
                            .settings_form_keybindings
                            .bindings
                            .get(&action)
                            .map(|b| b.display())
                            .unwrap_or_else(|| "Unbound".to_string());

                        // Highlight if we're capturing for this action
                        if self.capturing_key_for == Some(action) {
                            ui.colored_label(egui::Color32::YELLOW, "...");
                        } else {
                            ui.monospace(&binding_display);
                        }

                        // Context selector
                        let current_context = self
                            .settings_form_keybindings
                            .bindings
                            .get(&action)
                            .map(|b| b.context)
                            .unwrap_or(action.default_context());

                        let mut selected_context = current_context;
                        egui::ComboBox::from_id_salt(format!("context_{:?}", action))
                            .width(120.0)
                            .selected_text(selected_context.label())
                            .show_ui(ui, |ui| {
                                for ctx in KeyContext::all() {
                                    ui.selectable_value(&mut selected_context, *ctx, ctx.label());
                                }
                            });

                        // Update context if changed
                        if selected_context != current_context {
                            if let Some(binding) =
                                self.settings_form_keybindings.bindings.get_mut(&action)
                            {
                                binding.context = selected_context;
                            }
                        }

                        if self.capturing_key_for.is_none() {
                            if ui.button("Change").clicked() {
                                self.capturing_key_for = Some(action);
                            }
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if ui.button("Reset to Defaults").clicked() {
                self.settings_form_keybindings = KeyBindings::default();
            }

            ui.add_space(20.0);
            ui.label("Context: Where the shortcut is active");
        });
    }

    fn show_settings_ids_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Requirement ID Configuration");
        ui.add_space(10.0);

        // Validate current settings against proposed changes
        let validation = self.store.validate_id_config_change(
            &self.settings_form_id_format,
            &self.settings_form_numbering,
            self.settings_form_digits,
        );

        egui::Grid::new("settings_project_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                // ID Format selection
                ui.label("ID Format:");
                egui::ComboBox::from_id_salt("settings_id_format_combo")
                    .selected_text(match self.settings_form_id_format {
                        IdFormat::SingleLevel => "Single Level (PREFIX-NNN)",
                        IdFormat::TwoLevel => "Two Level (FEATURE-TYPE-NNN)",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.settings_form_id_format,
                            IdFormat::SingleLevel,
                            "Single Level (PREFIX-NNN)"
                        ).on_hover_text("e.g., AUTH-001, FR-002");
                        ui.selectable_value(
                            &mut self.settings_form_id_format,
                            IdFormat::TwoLevel,
                            "Two Level (FEATURE-TYPE-NNN)"
                        ).on_hover_text("e.g., AUTH-FR-001, PAY-NFR-001");
                    });
                ui.end_row();

                // Numbering Strategy selection
                ui.label("Numbering:");
                egui::ComboBox::from_id_salt("settings_numbering_combo")
                    .selected_text(match self.settings_form_numbering {
                        NumberingStrategy::Global => "Global Sequential",
                        NumberingStrategy::PerPrefix => "Per Prefix",
                        NumberingStrategy::PerFeatureType => "Per Feature+Type",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.settings_form_numbering,
                            NumberingStrategy::Global,
                            "Global Sequential"
                        ).on_hover_text("All requirements share one counter: AUTH-001, FR-002, PAY-003");
                        ui.selectable_value(
                            &mut self.settings_form_numbering,
                            NumberingStrategy::PerPrefix,
                            "Per Prefix"
                        ).on_hover_text("Each prefix has its own counter: AUTH-001, FR-001, PAY-001");
                        // Only show PerFeatureType for TwoLevel format
                        if self.settings_form_id_format == IdFormat::TwoLevel {
                            ui.selectable_value(
                                &mut self.settings_form_numbering,
                                NumberingStrategy::PerFeatureType,
                                "Per Feature+Type"
                            ).on_hover_text("Each feature+type combo has its own counter: AUTH-FR-001, AUTH-NFR-001");
                        }
                    });
                ui.end_row();

                // Number of digits
                ui.label("Digits:");
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.settings_form_digits, 1..=6)
                        .step_by(1.0));
                    ui.label(format!("(e.g., {:0>width$})", 1, width = self.settings_form_digits as usize));
                });
                ui.end_row();
            });

        ui.add_space(10.0);

        // Show validation status
        if let Some(error) = &validation.error {
            ui.colored_label(egui::Color32::RED, format!("⚠ {}", error));
            ui.add_space(5.0);
        }

        if let Some(warning) = &validation.warning {
            ui.colored_label(egui::Color32::YELLOW, format!("ℹ {}", warning));
            ui.add_space(5.0);
        }

        // Migration button (only show if changes are valid and migration is possible)
        if validation.valid && validation.can_migrate && validation.affected_count > 0 {
            ui.add_space(5.0);
            if ui
                .button("🔄 Migrate Existing IDs")
                .on_hover_text("Update all existing requirement IDs to match the new format")
                .clicked()
            {
                self.pending_migration = Some((
                    self.settings_form_id_format.clone(),
                    self.settings_form_numbering.clone(),
                    self.settings_form_digits,
                ));
                self.show_migration_dialog = true;
            }
        }

        ui.add_space(15.0);
        ui.separator();
        ui.add_space(10.0);

        // Show examples based on current settings
        ui.heading("Example IDs");
        ui.add_space(5.0);

        let width = self.settings_form_digits as usize;
        let examples = match self.settings_form_id_format {
            IdFormat::SingleLevel => {
                vec![
                    format!("AUTH-{:0>width$}", 1, width = width),
                    format!("FR-{:0>width$}", 2, width = width),
                    format!("PAY-{:0>width$}", 3, width = width),
                ]
            }
            IdFormat::TwoLevel => {
                vec![
                    format!("AUTH-FR-{:0>width$}", 1, width = width),
                    format!("AUTH-NFR-{:0>width$}", 2, width = width),
                    format!("PAY-FR-{:0>width$}", 3, width = width),
                ]
            }
        };

        for example in examples {
            ui.monospace(&example);
        }

        ui.add_space(10.0);
        if validation.affected_count == 0 {
            ui.colored_label(
                egui::Color32::from_rgb(180, 180, 100),
                "Note: Changes will apply to new requirements only.",
            );
        }

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // ID Prefix Management Section
            ui.heading("ID Prefix Management");
            ui.add_space(5.0);

            // Toggle for restricting prefixes
            let mut restrict = self.store.restrict_prefixes;
            if ui
                .checkbox(&mut restrict, "Restrict prefixes to allowed list")
                .on_hover_text("When enabled, users must select from the allowed prefixes list. When disabled, users can enter any valid prefix.")
                .changed()
            {
                self.store.restrict_prefixes = restrict;
                self.save();
            }

            ui.add_space(5.0);

            // Show used prefixes
            let used_prefixes = self.store.get_used_prefixes();

            ui.label(format!(
                "Prefixes in use: {}",
                if used_prefixes.is_empty() {
                    "none".to_string()
                } else {
                    used_prefixes.join(", ")
                }
            ));

            ui.add_space(5.0);
            ui.label("Allowed Prefixes:");

            // Add new prefix input
            ui.horizontal(|ui| {
                ui.label("Add prefix:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_prefix_input)
                        .desired_width(80.0)
                        .hint_text("e.g., SEC"),
                );

                if ui.button("Add").clicked() && !self.new_prefix_input.is_empty() {
                    let prefix = self.new_prefix_input.to_uppercase();
                    // Validate: must be uppercase letters only
                    if prefix.chars().all(|c| c.is_ascii_uppercase()) {
                        self.store.add_allowed_prefix(&prefix);
                        self.save();
                        self.new_prefix_input.clear();
                    }
                }
            });

            ui.add_space(5.0);

            // List allowed prefixes with delete buttons
            if self.store.allowed_prefixes.is_empty() {
                ui.label("No prefixes explicitly allowed (all valid prefixes permitted).");
            } else {
                let prefixes_to_show: Vec<String> = self.store.allowed_prefixes.clone();
                let mut to_remove: Option<String> = None;

                ui.horizontal_wrapped(|ui| {
                    for prefix in &prefixes_to_show {
                        let in_use = used_prefixes.contains(prefix);
                        ui.horizontal(|ui| {
                            ui.label(prefix);
                            if in_use {
                                ui.small("[in use]");
                            }
                            if ui
                                .small_button("×")
                                .on_hover_text("Remove from allowed list")
                                .clicked()
                            {
                                to_remove = Some(prefix.clone());
                            }
                        });
                    }
                });

                if let Some(prefix) = to_remove {
                    self.store.remove_allowed_prefix(&prefix);
                    self.save();
                }
            }
        });
    }

    fn show_settings_relationships_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Relationship Definitions");
        ui.add_space(5.0);
        ui.label("Configure relationship types and their constraints.");
        ui.add_space(10.0);

        // Add new relationship button
        if !self.show_rel_def_form {
            if ui.button("➕ Add Relationship Type").clicked() {
                self.show_rel_def_form = true;
                self.editing_rel_def = None;
                self.clear_rel_def_form();
            }
        }

        // Show form if active
        if self.show_rel_def_form {
            ui.add_space(10.0);
            self.show_rel_def_form_ui(ui);
            ui.add_space(10.0);
        }

        ui.separator();
        ui.add_space(10.0);

        // List existing relationship definitions
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                // Collect definitions to avoid borrow issues
                let definitions: Vec<_> = self.store.get_relationship_definitions().to_vec();

                for def in definitions {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            // Display name and built-in badge
                            ui.strong(&def.display_name);
                            if def.built_in {
                                ui.label(
                                    egui::RichText::new("[built-in]")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Delete button (only for non-built-in)
                                    if !def.built_in {
                                        if ui.small_button("🗑").on_hover_text("Delete").clicked()
                                        {
                                            if let Err(e) =
                                                self.store.remove_relationship_definition(&def.name)
                                            {
                                                self.message = Some((
                                                    format!("Failed to remove: {}", e),
                                                    true,
                                                ));
                                            } else {
                                                self.save();
                                                self.message = Some((
                                                    "Relationship definition removed".to_string(),
                                                    false,
                                                ));
                                            }
                                        }
                                    }
                                    // Edit button
                                    if ui.small_button("✏").on_hover_text("Edit").clicked() {
                                        self.editing_rel_def = Some(def.name.clone());
                                        self.load_rel_def_form(&def);
                                        self.show_rel_def_form = true;
                                    }
                                },
                            );
                        });

                        // Details
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&def.name).small().monospace());
                            ui.label("|");
                            if def.symmetric {
                                ui.label("↔ symmetric");
                            } else if let Some(ref inv) = def.inverse {
                                ui.label(format!("↔ {}", inv));
                            }
                            ui.label("|");
                            ui.label(format!("{}", def.cardinality));
                        });

                        if !def.description.is_empty() {
                            ui.label(
                                egui::RichText::new(&def.description)
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                        }

                        // Show type constraints if any
                        if !def.source_types.is_empty() || !def.target_types.is_empty() {
                            ui.horizontal(|ui| {
                                if !def.source_types.is_empty() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "From: {}",
                                            def.source_types.join(", ")
                                        ))
                                        .small(),
                                    );
                                }
                                if !def.target_types.is_empty() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "To: {}",
                                            def.target_types.join(", ")
                                        ))
                                        .small(),
                                    );
                                }
                            });
                        }

                        // Show color if set
                        if let Some(ref color) = def.color {
                            ui.horizontal(|ui| {
                                // Try to parse and display color swatch
                                if let Some(c) = parse_hex_color(color) {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(16.0, 16.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 2.0, c);
                                }
                                ui.label(egui::RichText::new(color).small());
                            });
                        }
                    });
                    ui.add_space(5.0);
                }
            });
    }

    fn clear_rel_def_form(&mut self) {
        self.rel_def_form_name.clear();
        self.rel_def_form_display_name.clear();
        self.rel_def_form_description.clear();
        self.rel_def_form_inverse.clear();
        self.rel_def_form_symmetric = false;
        self.rel_def_form_cardinality = Cardinality::default();
        self.rel_def_form_source_types.clear();
        self.rel_def_form_target_types.clear();
        self.rel_def_form_color.clear();
    }

    fn load_rel_def_form(&mut self, def: &RelationshipDefinition) {
        self.rel_def_form_name = def.name.clone();
        self.rel_def_form_display_name = def.display_name.clone();
        self.rel_def_form_description = def.description.clone();
        self.rel_def_form_inverse = def.inverse.clone().unwrap_or_default();
        self.rel_def_form_symmetric = def.symmetric;
        self.rel_def_form_cardinality = def.cardinality.clone();
        self.rel_def_form_source_types = def.source_types.join(", ");
        self.rel_def_form_target_types = def.target_types.join(", ");
        self.rel_def_form_color = def.color.clone().unwrap_or_default();
    }

    fn show_rel_def_form_ui(&mut self, ui: &mut egui::Ui) {
        let is_editing = self.editing_rel_def.is_some();
        let is_built_in = if let Some(ref name) = self.editing_rel_def {
            self.store
                .get_relationship_definition(name)
                .map(|d| d.built_in)
                .unwrap_or(false)
        } else {
            false
        };

        ui.group(|ui| {
            let title = if is_editing {
                if is_built_in {
                    "Edit Built-in Relationship (limited)"
                } else {
                    "Edit Relationship"
                }
            } else {
                "Add Relationship Type"
            };
            ui.heading(title);
            ui.add_space(5.0);

            egui::Grid::new("rel_def_form_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    // Name (readonly for built-in)
                    ui.label("Name:");
                    if is_editing {
                        ui.label(&self.rel_def_form_name);
                    } else {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.rel_def_form_name)
                                .hint_text("e.g., blocks"),
                        );
                    }
                    ui.end_row();

                    // Display name (editable)
                    ui.label("Display Name:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rel_def_form_display_name)
                            .hint_text("e.g., Blocks"),
                    );
                    ui.end_row();

                    // Description (editable)
                    ui.label("Description:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rel_def_form_description)
                            .hint_text("What this relationship means"),
                    );
                    ui.end_row();

                    // For non-built-in: inverse, symmetric, cardinality
                    if !is_built_in {
                        ui.label("Inverse:");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.rel_def_form_inverse)
                                    .hint_text("e.g., blocked_by")
                                    .desired_width(120.0),
                            );
                            ui.checkbox(&mut self.rel_def_form_symmetric, "Symmetric");
                        });
                        ui.end_row();

                        ui.label("Cardinality:");
                        egui::ComboBox::from_id_salt("cardinality_combo")
                            .selected_text(format!("{}", self.rel_def_form_cardinality))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.rel_def_form_cardinality,
                                    Cardinality::ManyToMany,
                                    "N:N (Many to Many)",
                                );
                                ui.selectable_value(
                                    &mut self.rel_def_form_cardinality,
                                    Cardinality::OneToMany,
                                    "1:N (One to Many)",
                                );
                                ui.selectable_value(
                                    &mut self.rel_def_form_cardinality,
                                    Cardinality::ManyToOne,
                                    "N:1 (Many to One)",
                                );
                                ui.selectable_value(
                                    &mut self.rel_def_form_cardinality,
                                    Cardinality::OneToOne,
                                    "1:1 (One to One)",
                                );
                            });
                        ui.end_row();
                    }

                    // Source types (editable)
                    ui.label("Source Types:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rel_def_form_source_types)
                            .hint_text("Functional, System (comma-separated, empty = all)"),
                    );
                    ui.end_row();

                    // Target types (editable)
                    ui.label("Target Types:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rel_def_form_target_types)
                            .hint_text("Functional, System (comma-separated, empty = all)"),
                    );
                    ui.end_row();

                    // Color (editable)
                    ui.label("Color:");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.rel_def_form_color)
                                .hint_text("#ff6b6b")
                                .desired_width(80.0),
                        );
                        // Show color preview
                        if let Some(c) = parse_hex_color(&self.rel_def_form_color) {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, c);
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    self.save_rel_def_form();
                }
                if ui.button("❌ Cancel").clicked() {
                    self.show_rel_def_form = false;
                    self.editing_rel_def = None;
                }
            });
        });
    }

    fn save_rel_def_form(&mut self) {
        // Parse source/target types
        let source_types: Vec<String> = self
            .rel_def_form_source_types
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let target_types: Vec<String> = self
            .rel_def_form_target_types
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let color = if self.rel_def_form_color.trim().is_empty() {
            None
        } else {
            Some(self.rel_def_form_color.trim().to_string())
        };

        let inverse = if self.rel_def_form_inverse.trim().is_empty() {
            None
        } else {
            Some(self.rel_def_form_inverse.trim().to_lowercase())
        };

        if let Some(ref edit_name) = self.editing_rel_def {
            // Update existing
            let is_built_in = self
                .store
                .get_relationship_definition(edit_name)
                .map(|d| d.built_in)
                .unwrap_or(false);

            let mut updated = if is_built_in {
                // For built-in, start from existing and only update allowed fields
                self.store
                    .get_relationship_definition(edit_name)
                    .cloned()
                    .unwrap_or_else(|| {
                        RelationshipDefinition::new(
                            &self.rel_def_form_name,
                            &self.rel_def_form_display_name,
                        )
                    })
            } else {
                RelationshipDefinition::new(
                    &self.rel_def_form_name,
                    &self.rel_def_form_display_name,
                )
            };

            updated.display_name = self.rel_def_form_display_name.clone();
            updated.description = self.rel_def_form_description.clone();
            updated.source_types = source_types;
            updated.target_types = target_types;
            updated.color = color;

            if !is_built_in {
                updated.inverse = inverse;
                updated.symmetric = self.rel_def_form_symmetric;
                updated.cardinality = self.rel_def_form_cardinality.clone();
            }

            match self
                .store
                .update_relationship_definition(edit_name, updated)
            {
                Ok(()) => {
                    self.save();
                    self.message = Some(("Relationship definition updated".to_string(), false));
                    self.show_rel_def_form = false;
                    self.editing_rel_def = None;
                }
                Err(e) => {
                    self.message = Some((format!("Failed to update: {}", e), true));
                }
            }
        } else {
            // Add new
            if self.rel_def_form_name.trim().is_empty() {
                self.message = Some(("Name is required".to_string(), true));
                return;
            }

            let mut new_def = RelationshipDefinition::new(
                &self.rel_def_form_name.trim().to_lowercase(),
                if self.rel_def_form_display_name.trim().is_empty() {
                    &self.rel_def_form_name
                } else {
                    &self.rel_def_form_display_name
                },
            );

            new_def.description = self.rel_def_form_description.clone();
            new_def.inverse = inverse;
            new_def.symmetric = self.rel_def_form_symmetric;
            new_def.cardinality = self.rel_def_form_cardinality.clone();
            new_def.source_types = source_types;
            new_def.target_types = target_types;
            new_def.color = color;

            match self.store.add_relationship_definition(new_def) {
                Ok(()) => {
                    self.save();
                    self.message = Some(("Relationship definition added".to_string(), false));
                    self.show_rel_def_form = false;
                }
                Err(e) => {
                    self.message = Some((format!("Failed to add: {}", e), true));
                }
            }
        }
    }

    fn show_settings_reactions_tab(&mut self, ui: &mut egui::Ui) {
        use aida_core::ReactionDefinition;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Comment Reactions");
            ui.add_space(5.0);
            ui.label("Configure emoji reactions that can be added to comments.");
            ui.add_space(10.0);

            // Add new reaction button
            if ui.button("➕ Add Reaction").clicked() {
                self.show_reaction_def_form = true;
                self.editing_reaction_def = None;
                self.reaction_def_form_name.clear();
                self.reaction_def_form_emoji.clear();
                self.reaction_def_form_label.clear();
                self.reaction_def_form_description.clear();
            }

            ui.add_space(10.0);

            // Reaction form (inline)
            if self.show_reaction_def_form {
                ui.group(|ui| {
                    let title = if self.editing_reaction_def.is_some() {
                        "Edit Reaction"
                    } else {
                        "Add Reaction"
                    };
                    ui.heading(title);

                    egui::Grid::new("reaction_form_grid")
                        .num_columns(2)
                        .spacing([10.0, 5.0])
                        .show(ui, |ui| {
                            ui.label("Name (ID):");
                            let name_editable = self.editing_reaction_def.is_none();
                            ui.add_enabled(
                                name_editable,
                                egui::TextEdit::singleline(&mut self.reaction_def_form_name)
                                    .hint_text("e.g., thumbs_up"),
                            );
                            ui.end_row();

                            ui.label("Emoji:");
                            ui.text_edit_singleline(&mut self.reaction_def_form_emoji);
                            ui.end_row();

                            ui.label("Label:");
                            ui.text_edit_singleline(&mut self.reaction_def_form_label);
                            ui.end_row();

                            ui.label("Description:");
                            ui.text_edit_singleline(&mut self.reaction_def_form_description);
                            ui.end_row();
                        });

                    ui.add_space(5.0);

                    ui.horizontal(|ui| {
                        let can_save = !self.reaction_def_form_name.is_empty()
                            && !self.reaction_def_form_emoji.is_empty()
                            && !self.reaction_def_form_label.is_empty();

                        if ui
                            .add_enabled(can_save, egui::Button::new("💾 Save"))
                            .clicked()
                        {
                            if let Some(ref editing_name) = self.editing_reaction_def.clone() {
                                // Update existing
                                if let Some(def) = self
                                    .store
                                    .reaction_definitions
                                    .iter_mut()
                                    .find(|d| &d.name == editing_name)
                                {
                                    def.emoji = self.reaction_def_form_emoji.clone();
                                    def.label = self.reaction_def_form_label.clone();
                                    def.description =
                                        if self.reaction_def_form_description.is_empty() {
                                            None
                                        } else {
                                            Some(self.reaction_def_form_description.clone())
                                        };
                                }
                            } else {
                                // Add new
                                let mut new_def = ReactionDefinition::new(
                                    self.reaction_def_form_name.clone(),
                                    self.reaction_def_form_emoji.clone(),
                                    self.reaction_def_form_label.clone(),
                                );
                                if !self.reaction_def_form_description.is_empty() {
                                    new_def.description =
                                        Some(self.reaction_def_form_description.clone());
                                }
                                self.store.reaction_definitions.push(new_def);
                            }
                            self.save();
                            self.show_reaction_def_form = false;
                            self.editing_reaction_def = None;
                        }

                        if ui.button("Cancel").clicked() {
                            self.show_reaction_def_form = false;
                            self.editing_reaction_def = None;
                        }
                    });
                });

                ui.add_space(10.0);
            }

            // List existing reactions
            ui.heading("Defined Reactions");
            ui.add_space(5.0);

            // Clone to avoid borrow issues
            let reactions: Vec<_> = self.store.reaction_definitions.clone();

            if reactions.is_empty() {
                ui.label("No reactions defined.");
            } else {
                egui::Grid::new("reactions_table")
                    .num_columns(5)
                    .striped(true)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        // Header
                        ui.strong("Emoji");
                        ui.strong("Name");
                        ui.strong("Label");
                        ui.strong("Description");
                        ui.strong("Actions");
                        ui.end_row();

                        let mut to_delete: Option<String> = None;

                        for def in &reactions {
                            ui.label(&def.emoji);
                            ui.horizontal(|ui| {
                                ui.label(&def.name);
                                if def.built_in {
                                    ui.small("[built-in]");
                                }
                            });
                            ui.label(&def.label);
                            ui.label(def.description.as_deref().unwrap_or("-"));

                            ui.horizontal(|ui| {
                                if ui.small_button("✏").on_hover_text("Edit").clicked() {
                                    self.editing_reaction_def = Some(def.name.clone());
                                    self.reaction_def_form_name = def.name.clone();
                                    self.reaction_def_form_emoji = def.emoji.clone();
                                    self.reaction_def_form_label = def.label.clone();
                                    self.reaction_def_form_description =
                                        def.description.clone().unwrap_or_default();
                                    self.show_reaction_def_form = true;
                                }
                                // Only allow deletion of non-built-in reactions
                                if !def.built_in {
                                    if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                                        to_delete = Some(def.name.clone());
                                    }
                                }
                            });
                            ui.end_row();
                        }

                        // Handle deletion outside the iteration
                        if let Some(name) = to_delete {
                            self.store.reaction_definitions.retain(|d| d.name != name);
                            self.save();
                        }
                    });
            }

            ui.add_space(15.0);

            // Reset to defaults button
            if ui
                .button("↺ Reset to Defaults")
                .on_hover_text("Restore built-in reactions")
                .clicked()
            {
                self.store.reaction_definitions = aida_core::default_reaction_definitions();
                self.save();
            }
        });
    }

    fn show_settings_type_definitions_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Requirement Type Definitions");
            ui.add_space(5.0);
            ui.label("Configure requirement types, their available statuses, and custom fields.");
            ui.add_space(10.0);

            // Show type definition form if editing/adding
            if self.show_type_def_form {
                self.show_type_definition_form(ui);
                return;
            }

            // Add new type button
            if ui.button("➕ Add New Type").clicked() {
                self.editing_type_def = None;
                self.type_def_form_name.clear();
                self.type_def_form_display_name.clear();
                self.type_def_form_description.clear();
                self.type_def_form_prefix.clear();
                self.type_def_form_statuses = vec![
                    "Draft".to_string(),
                    "Approved".to_string(),
                    "Completed".to_string(),
                    "Rejected".to_string(),
                ];
                self.type_def_form_fields.clear();
                self.show_type_def_form = true;
            }

            ui.add_space(10.0);

            // Clone type definitions to avoid borrow issues
            let type_defs = self.store.type_definitions.clone();

            if type_defs.is_empty() {
                ui.label("No type definitions configured.");
            } else {
                let mut type_to_delete: Option<String> = None;
                let mut type_to_edit: Option<String> = None;
                let mut type_to_reset: Option<String> = None;

                for type_def in &type_defs {
                    let header = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        egui::Id::new(format!("type_def_{}", type_def.name)),
                        false,
                    );

                    header
                        .show_header(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} {}",
                                    if type_def.built_in { "📦" } else { "📝" },
                                    &type_def.display_name
                                ));

                                // Edit button
                                if ui.small_button("✏").on_hover_text("Edit type").clicked() {
                                    type_to_edit = Some(type_def.name.clone());
                                }

                                // Reset to default button (only for built-in types)
                                if type_def.built_in {
                                    if ui
                                        .small_button("↺")
                                        .on_hover_text("Reset to default")
                                        .clicked()
                                    {
                                        type_to_reset = Some(type_def.name.clone());
                                    }
                                } else {
                                    // Delete button (only for custom types)
                                    if ui.small_button("🗑").on_hover_text("Delete type").clicked()
                                    {
                                        type_to_delete = Some(type_def.name.clone());
                                    }
                                }
                            });
                        })
                        .body(|ui| {
                            egui::Grid::new(format!("type_def_grid_{}", type_def.name))
                                .num_columns(2)
                                .spacing([10.0, 5.0])
                                .show(ui, |ui| {
                                    ui.label("Internal Name:");
                                    ui.label(&type_def.name);
                                    ui.end_row();

                                    if let Some(ref prefix) = type_def.prefix {
                                        ui.label("ID Prefix:");
                                        ui.label(prefix);
                                        ui.end_row();
                                    }

                                    if let Some(ref desc) = type_def.description {
                                        ui.label("Description:");
                                        ui.label(desc);
                                        ui.end_row();
                                    }

                                    if type_def.built_in {
                                        ui.label("");
                                        ui.small("[Built-in type]");
                                        ui.end_row();
                                    }
                                });

                            // Statuses section
                            if !type_def.statuses.is_empty() {
                                ui.add_space(5.0);
                                ui.label("Available Statuses:");
                                ui.horizontal_wrapped(|ui| {
                                    for status in &type_def.statuses {
                                        ui.label(format!("• {}", status));
                                    }
                                });
                            }

                            // Custom fields section
                            if !type_def.custom_fields.is_empty() {
                                ui.add_space(5.0);
                                ui.label("Custom Fields:");
                                egui::Grid::new(format!("custom_fields_grid_{}", type_def.name))
                                    .num_columns(4)
                                    .striped(true)
                                    .spacing([10.0, 3.0])
                                    .show(ui, |ui| {
                                        ui.strong("Field");
                                        ui.strong("Type");
                                        ui.strong("Required");
                                        ui.strong("Options/Default");
                                        ui.end_row();

                                        for field in &type_def.custom_fields {
                                            ui.label(&field.label);
                                            ui.label(Self::field_type_display(&field.field_type));
                                            ui.label(if field.required { "Yes" } else { "No" });
                                            // Show options or default
                                            let extra_info = if !field.options.is_empty() {
                                                field.options.join(", ")
                                            } else if let Some(ref def) = field.default_value {
                                                format!("Default: {}", def)
                                            } else {
                                                "-".to_string()
                                            };
                                            ui.label(extra_info);
                                            ui.end_row();
                                        }
                                    });
                            }
                        });
                    ui.add_space(5.0);
                }

                // Handle edit action
                if let Some(name) = type_to_edit {
                    if let Some(type_def) =
                        self.store.type_definitions.iter().find(|t| t.name == name)
                    {
                        self.editing_type_def = Some(name);
                        self.type_def_form_name = type_def.name.clone();
                        self.type_def_form_display_name = type_def.display_name.clone();
                        self.type_def_form_description =
                            type_def.description.clone().unwrap_or_default();
                        self.type_def_form_prefix = type_def.prefix.clone().unwrap_or_default();
                        self.type_def_form_statuses = type_def.statuses.clone();
                        self.type_def_form_fields = type_def.custom_fields.clone();
                        self.show_type_def_form = true;
                    }
                }

                // Handle reset action
                if let Some(name) = type_to_reset {
                    let defaults = aida_core::default_type_definitions();
                    if let Some(default_def) = defaults.iter().find(|t| t.name == name) {
                        if let Some(idx) = self
                            .store
                            .type_definitions
                            .iter()
                            .position(|t| t.name == name)
                        {
                            self.store.type_definitions[idx] = default_def.clone();
                            self.save();
                        }
                    }
                }

                // Handle delete action
                if let Some(name) = type_to_delete {
                    // Check if any requirements use this type
                    let in_use = self
                        .store
                        .requirements
                        .iter()
                        .any(|r| format!("{:?}", r.req_type) == name);
                    if in_use {
                        self.message = Some((
                            format!(
                                "Cannot delete '{}': type is in use by existing requirements",
                                name
                            ),
                            true,
                        ));
                    } else {
                        self.store.type_definitions.retain(|t| t.name != name);
                        self.save();
                    }
                }
            }

            ui.add_space(15.0);

            // Reset all to defaults button
            if ui
                .button("↺ Reset All to Defaults")
                .on_hover_text("Restore all built-in type definitions")
                .clicked()
            {
                self.store.type_definitions = aida_core::default_type_definitions();
                self.save();
            }
        });
    }

    fn field_type_display(field_type: &CustomFieldType) -> &'static str {
        match field_type {
            CustomFieldType::Text => "Text",
            CustomFieldType::TextArea => "Text Area",
            CustomFieldType::Select => "Select",
            CustomFieldType::Boolean => "Boolean",
            CustomFieldType::Date => "Date",
            CustomFieldType::User => "User Reference",
            CustomFieldType::Requirement => "Requirement Reference",
            CustomFieldType::Number => "Number",
        }
    }

    fn show_type_definition_form(&mut self, ui: &mut egui::Ui) {
        let is_editing = self.editing_type_def.is_some();
        let title = if is_editing {
            format!("Edit Type: {}", self.editing_type_def.as_ref().unwrap())
        } else {
            "Add New Type".to_string()
        };

        ui.heading(&title);
        ui.add_space(10.0);

        // Show custom field form if editing/adding a field
        if self.show_field_form {
            self.show_custom_field_form(ui);
            return;
        }

        egui::Grid::new("type_def_form_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Name field (only editable for new types)
                ui.label("Internal Name:");
                if is_editing {
                    ui.label(&self.type_def_form_name);
                } else {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.type_def_form_name)
                            .hint_text("e.g., BugReport")
                            .desired_width(200.0),
                    );
                }
                ui.end_row();

                // Display name
                ui.label("Display Name:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.type_def_form_display_name)
                        .hint_text("e.g., Bug Report")
                        .desired_width(200.0),
                );
                ui.end_row();

                // Description
                ui.label("Description:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.type_def_form_description)
                        .hint_text("Optional description")
                        .desired_width(300.0),
                );
                ui.end_row();

                // ID Prefix
                ui.label("ID Prefix:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.type_def_form_prefix)
                        .hint_text("e.g., BUG")
                        .desired_width(100.0),
                );
                ui.end_row();
            });

        ui.add_space(15.0);

        // Statuses section
        ui.heading("Statuses");
        ui.add_space(5.0);

        // Display current statuses with remove buttons
        let mut status_to_remove: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (idx, status) in self.type_def_form_statuses.iter().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(status);
                        if ui
                            .small_button("✕")
                            .on_hover_text("Remove status")
                            .clicked()
                        {
                            status_to_remove = Some(idx);
                        }
                    });
                });
            }
        });

        if let Some(idx) = status_to_remove {
            // Check if status is in use before removing
            let status_name = &self.type_def_form_statuses[idx];
            if let Some(ref editing_name) = self.editing_type_def {
                let in_use = self.store.requirements.iter().any(|r| {
                    format!("{:?}", r.req_type) == *editing_name
                        && r.custom_status.as_ref() == Some(status_name)
                });
                if in_use {
                    self.message = Some((
                        format!("Cannot remove '{}': status is in use", status_name),
                        true,
                    ));
                } else {
                    self.type_def_form_statuses.remove(idx);
                }
            } else {
                self.type_def_form_statuses.remove(idx);
            }
        }

        // Add new status
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_status_input)
                    .hint_text("New status name")
                    .desired_width(150.0),
            );
            if ui.button("Add Status").clicked() && !self.new_status_input.is_empty() {
                if !self.type_def_form_statuses.contains(&self.new_status_input) {
                    self.type_def_form_statuses
                        .push(self.new_status_input.clone());
                    self.new_status_input.clear();
                }
            }
        });

        ui.add_space(15.0);

        // Custom fields section
        ui.heading("Custom Fields");
        ui.add_space(5.0);

        if self.type_def_form_fields.is_empty() {
            ui.label("No custom fields defined.");
        } else {
            let mut field_to_remove: Option<usize> = None;
            let mut field_to_edit: Option<usize> = None;

            egui::Grid::new("type_def_fields_grid")
                .num_columns(5)
                .striped(true)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    ui.strong("Field");
                    ui.strong("Type");
                    ui.strong("Required");
                    ui.strong("Options");
                    ui.strong("Actions");
                    ui.end_row();

                    for (idx, field) in self.type_def_form_fields.iter().enumerate() {
                        ui.label(&field.label);
                        ui.label(Self::field_type_display(&field.field_type));
                        ui.label(if field.required { "Yes" } else { "No" });
                        let extra = if !field.options.is_empty() {
                            field.options.join(", ")
                        } else if let Some(ref def) = field.default_value {
                            format!("Default: {}", def)
                        } else {
                            "-".to_string()
                        };
                        ui.label(extra);
                        ui.horizontal(|ui| {
                            if ui.small_button("✏").on_hover_text("Edit field").clicked() {
                                field_to_edit = Some(idx);
                            }
                            if ui.small_button("🗑").on_hover_text("Remove field").clicked() {
                                field_to_remove = Some(idx);
                            }
                        });
                        ui.end_row();
                    }
                });

            if let Some(idx) = field_to_edit {
                let field = &self.type_def_form_fields[idx];
                self.editing_field_idx = Some(idx);
                self.field_form_name = field.name.clone();
                self.field_form_label = field.label.clone();
                self.field_form_type = field.field_type.clone();
                self.field_form_required = field.required;
                self.field_form_options = field.options.join(", ");
                self.field_form_default = field.default_value.clone().unwrap_or_default();
                self.show_field_form = true;
            }

            if let Some(idx) = field_to_remove {
                // Check if field is in use before removing
                let field_name = &self.type_def_form_fields[idx].name;
                if let Some(ref editing_name) = self.editing_type_def {
                    let in_use = self.store.requirements.iter().any(|r| {
                        format!("{:?}", r.req_type) == *editing_name
                            && r.custom_fields.contains_key(field_name)
                    });
                    if in_use {
                        self.message = Some((
                            format!("Cannot remove '{}': field is in use", field_name),
                            true,
                        ));
                    } else {
                        self.type_def_form_fields.remove(idx);
                    }
                } else {
                    self.type_def_form_fields.remove(idx);
                }
            }
        }

        ui.add_space(5.0);
        if ui.button("➕ Add Field").clicked() {
            self.editing_field_idx = None;
            self.field_form_name.clear();
            self.field_form_label.clear();
            self.field_form_type = CustomFieldType::Text;
            self.field_form_required = false;
            self.field_form_options.clear();
            self.field_form_default.clear();
            self.show_field_form = true;
        }

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(10.0);

        // Save/Cancel buttons
        ui.horizontal(|ui| {
            let can_save = !self.type_def_form_name.is_empty()
                && !self.type_def_form_display_name.is_empty()
                && !self.type_def_form_statuses.is_empty();

            if ui
                .add_enabled(can_save, egui::Button::new("💾 Save"))
                .clicked()
            {
                self.save_type_definition();
            }

            if ui.button("Cancel").clicked() {
                self.show_type_def_form = false;
            }
        });

        if self.type_def_form_name.is_empty() || self.type_def_form_display_name.is_empty() {
            ui.small("Name and display name are required.");
        }
        if self.type_def_form_statuses.is_empty() {
            ui.small("At least one status is required.");
        }
    }

    fn show_custom_field_form(&mut self, ui: &mut egui::Ui) {
        let is_editing = self.editing_field_idx.is_some();
        let title = if is_editing {
            "Edit Custom Field"
        } else {
            "Add Custom Field"
        };

        ui.heading(title);
        ui.add_space(10.0);

        egui::Grid::new("custom_field_form_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Field name (internal key)
                ui.label("Field Name:");
                if is_editing {
                    ui.label(&self.field_form_name);
                } else {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.field_form_name)
                            .hint_text("e.g., impact_level")
                            .desired_width(200.0),
                    );
                }
                ui.end_row();

                // Label (display name)
                ui.label("Label:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.field_form_label)
                        .hint_text("e.g., Impact Level")
                        .desired_width(200.0),
                );
                ui.end_row();

                // Field type
                ui.label("Type:");
                egui::ComboBox::from_id_salt("field_type_combo")
                    .selected_text(Self::field_type_display(&self.field_form_type))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::Text,
                            "Text",
                        );
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::TextArea,
                            "Text Area",
                        );
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::Select,
                            "Select",
                        );
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::Boolean,
                            "Boolean",
                        );
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::Date,
                            "Date",
                        );
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::Number,
                            "Number",
                        );
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::User,
                            "User Reference",
                        );
                        ui.selectable_value(
                            &mut self.field_form_type,
                            CustomFieldType::Requirement,
                            "Requirement Reference",
                        );
                    });
                ui.end_row();

                // Required checkbox
                ui.label("Required:");
                ui.checkbox(&mut self.field_form_required, "");
                ui.end_row();

                // Options (for Select type)
                if self.field_form_type == CustomFieldType::Select {
                    ui.label("Options:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.field_form_options)
                            .hint_text("Option1, Option2, Option3")
                            .desired_width(300.0),
                    );
                    ui.end_row();
                }

                // Default value
                ui.label("Default Value:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.field_form_default)
                        .hint_text("Optional default")
                        .desired_width(200.0),
                );
                ui.end_row();
            });

        ui.add_space(15.0);

        // Save/Cancel buttons
        ui.horizontal(|ui| {
            let can_save = !self.field_form_name.is_empty() && !self.field_form_label.is_empty();

            if ui
                .add_enabled(can_save, egui::Button::new("💾 Save Field"))
                .clicked()
            {
                let options: Vec<String> = if self.field_form_type == CustomFieldType::Select {
                    self.field_form_options
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    Vec::new()
                };

                let field = CustomFieldDefinition {
                    name: self.field_form_name.clone(),
                    label: self.field_form_label.clone(),
                    field_type: self.field_form_type.clone(),
                    required: self.field_form_required,
                    options,
                    default_value: if self.field_form_default.is_empty() {
                        None
                    } else {
                        Some(self.field_form_default.clone())
                    },
                    description: None,
                    order: 0,
                };

                if let Some(idx) = self.editing_field_idx {
                    self.type_def_form_fields[idx] = field;
                } else {
                    self.type_def_form_fields.push(field);
                }

                self.show_field_form = false;
            }

            if ui.button("Cancel").clicked() {
                self.show_field_form = false;
            }
        });
    }

    fn save_type_definition(&mut self) {
        use aida_core::CustomTypeDefinition;

        let type_def = CustomTypeDefinition {
            name: self.type_def_form_name.clone(),
            display_name: self.type_def_form_display_name.clone(),
            description: if self.type_def_form_description.is_empty() {
                None
            } else {
                Some(self.type_def_form_description.clone())
            },
            prefix: if self.type_def_form_prefix.is_empty() {
                None
            } else {
                Some(self.type_def_form_prefix.to_uppercase())
            },
            statuses: self.type_def_form_statuses.clone(),
            custom_fields: self.type_def_form_fields.clone(),
            built_in: if let Some(ref editing_name) = self.editing_type_def {
                // Preserve built_in status when editing
                self.store
                    .type_definitions
                    .iter()
                    .find(|t| &t.name == editing_name)
                    .map(|t| t.built_in)
                    .unwrap_or(false)
            } else {
                false
            },
            color: None,
        };

        if let Some(ref editing_name) = self.editing_type_def {
            // Update existing type
            if let Some(idx) = self
                .store
                .type_definitions
                .iter()
                .position(|t| &t.name == editing_name)
            {
                self.store.type_definitions[idx] = type_def;
            }
        } else {
            // Add new type
            self.store.type_definitions.push(type_def);
        }

        self.save();
        self.show_type_def_form = false;
    }

    fn show_settings_users_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("User Management");
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                if ui.button("➕ Add User").clicked() {
                    self.show_user_form = true;
                    self.editing_user_id = None;
                    self.user_form_name.clear();
                    self.user_form_email.clear();
                    self.user_form_handle.clear();
                }
                ui.checkbox(&mut self.show_archived_users, "Show Archived");
            });

            ui.add_space(5.0);

            // User form (inline)
            if self.show_user_form {
                ui.group(|ui| {
                    let title = if self.editing_user_id.is_some() {
                        "Edit User"
                    } else {
                        "Add User"
                    };
                    ui.label(title);

                    egui::Grid::new("user_form_grid")
                        .num_columns(2)
                        .spacing([10.0, 5.0])
                        .show(ui, |ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut self.user_form_name);
                            ui.end_row();

                            ui.label("Email:");
                            ui.text_edit_singleline(&mut self.user_form_email);
                            ui.end_row();

                            ui.label("Handle:");
                            ui.text_edit_singleline(&mut self.user_form_handle);
                            ui.end_row();
                        });

                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.editing_user_id.is_some() {
                                self.save_edited_user();
                            } else {
                                self.add_new_user();
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_user_form = false;
                            self.editing_user_id = None;
                        }
                    });
                });
                ui.add_space(5.0);
            }

            // Users table
            self.show_users_table(ui);
        });
    }

    fn show_settings_database_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Database Identity Section
            ui.heading("Database Identity");
            ui.add_space(5.0);

            // Name field
            ui.horizontal(|ui| {
                ui.label("Name:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.store.name)
                        .desired_width(300.0)
                        .hint_text("Short identifier (e.g., PROJ-A)"),
                );
                if response.lost_focus() {
                    self.save();
                }
            });

            ui.add_space(5.0);

            // Title field
            ui.horizontal(|ui| {
                ui.label("Title:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.store.title)
                        .desired_width(400.0)
                        .hint_text("One-line title for this database"),
                );
                if response.lost_focus() {
                    self.save();
                }
            });

            ui.add_space(5.0);

            // Description field (multiline)
            ui.label("Description:");
            let response = ui.add(
                egui::TextEdit::multiline(&mut self.store.description)
                    .desired_width(ui.available_width() - 20.0)
                    .desired_rows(4)
                    .hint_text("Detailed description of this requirements database..."),
            );
            if response.lost_focus() {
                self.save();
            }

            ui.add_space(5.0);
            ui.label("Window title: Name - Title (or whichever is set)");

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Database Management Section
            ui.heading("Database Management");
            ui.add_space(5.0);

            if ui.button("📦 Backup Database").clicked() {
                self.backup_database();
            }
            ui.label("Creates a timestamped backup of the requirements database.");

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Statistics Section
            ui.heading("Database Statistics");
            ui.add_space(5.0);

            let total_reqs = self.store.requirements.len();
            let archived_reqs = self
                .store
                .requirements
                .iter()
                .filter(|r| r.archived)
                .count();
            let active_reqs = total_reqs - archived_reqs;
            let total_users = self.store.users.len();
            let archived_users = self.store.users.iter().filter(|u| u.archived).count();

            egui::Grid::new("settings_stats_grid")
                .num_columns(2)
                .spacing([20.0, 5.0])
                .show(ui, |ui| {
                    ui.label("Total Requirements:");
                    ui.label(format!("{}", total_reqs));
                    ui.end_row();

                    ui.label("Active Requirements:");
                    ui.label(format!("{}", active_reqs));
                    ui.end_row();

                    ui.label("Archived Requirements:");
                    ui.label(format!("{}", archived_reqs));
                    ui.end_row();

                    ui.label("Total Users:");
                    ui.label(format!("{}", total_users));
                    ui.end_row();

                    ui.label("Archived Users:");
                    ui.label(format!("{}", archived_users));
                    ui.end_row();

                    ui.label("Database Path:");
                    ui.label(self.storage.path().display().to_string());
                    ui.end_row();
                });
        });
    }

    /// Show the theme editor dialog
    fn show_theme_editor_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_theme_editor {
            return;
        }

        // Apply the working theme so user sees changes live
        self.theme_editor_theme.apply(ctx);

        // Fixed dimensions for theme editor
        const THEME_EDITOR_WIDTH: f32 = 850.0;
        const THEME_EDITOR_HEIGHT: f32 = 500.0;
        const CONTENT_HEIGHT: f32 = 400.0;

        egui::Window::new("🎨 Theme Editor")
            .collapsible(false)
            .resizable(false)
            .fixed_size([THEME_EDITOR_WIDTH, THEME_EDITOR_HEIGHT])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 30.0]) // Offset down to avoid menu bar
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Left column: Categories (fixed width)
                    ui.allocate_ui_with_layout(
                        egui::vec2(140.0, CONTENT_HEIGHT),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.heading("Categories");
                            ui.add_space(5.0);
                            for cat in ThemeEditorCategory::all() {
                                if ui
                                    .selectable_label(
                                        self.theme_editor_category == *cat,
                                        cat.label(),
                                    )
                                    .clicked()
                                {
                                    self.theme_editor_category = *cat;
                                }
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(5.0);

                            // Base theme selector
                            ui.label("Base Theme:");
                            egui::ComboBox::from_id_salt("theme_editor_base")
                                .selected_text(self.theme_editor_theme.base.label())
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            self.theme_editor_theme.base == BaseTheme::Dark,
                                            "Dark",
                                        )
                                        .clicked()
                                    {
                                        self.theme_editor_theme.base = BaseTheme::Dark;
                                        self.theme_editor_theme.dark_mode = true;
                                    }
                                    if ui
                                        .selectable_label(
                                            self.theme_editor_theme.base == BaseTheme::Light,
                                            "Light",
                                        )
                                        .clicked()
                                    {
                                        self.theme_editor_theme.base = BaseTheme::Light;
                                        self.theme_editor_theme.dark_mode = false;
                                    }
                                });

                            ui.add_space(10.0);
                            if ui.button("Reset to Base").clicked() {
                                let name = self.theme_editor_theme.name.clone();
                                self.theme_editor_theme =
                                    CustomTheme::from_base(self.theme_editor_theme.base, name);
                            }
                        },
                    );

                    ui.separator();

                    // Center column: Property editors (fixed width)
                    ui.allocate_ui_with_layout(
                        egui::vec2(340.0, CONTENT_HEIGHT),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Theme Name:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.theme_editor_theme.name)
                                        .desired_width(200.0),
                                );
                            });
                            ui.add_space(10.0);
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_salt("theme_editor_properties_scroll")
                                .max_height(CONTENT_HEIGHT - 50.0)
                                .show(ui, |ui| {
                                    match self.theme_editor_category {
                                        ThemeEditorCategory::Backgrounds => {
                                            Self::show_theme_backgrounds(
                                                &mut self.theme_editor_theme,
                                                ui,
                                            );
                                        }
                                        ThemeEditorCategory::Text => {
                                            Self::show_theme_text(&mut self.theme_editor_theme, ui);
                                        }
                                        ThemeEditorCategory::Widgets => {
                                            Self::show_theme_widgets(
                                                &mut self.theme_editor_theme,
                                                ui,
                                            );
                                        }
                                        ThemeEditorCategory::Selection => {
                                            Self::show_theme_selection(
                                                &mut self.theme_editor_theme,
                                                ui,
                                            );
                                        }
                                        ThemeEditorCategory::Borders => {
                                            Self::show_theme_borders(
                                                &mut self.theme_editor_theme,
                                                ui,
                                            );
                                        }
                                        ThemeEditorCategory::Spacing => {
                                            Self::show_theme_spacing(
                                                &mut self.theme_editor_theme,
                                                ui,
                                            );
                                        }
                                    }
                                });
                        },
                    );

                    ui.separator();

                    // Right column: Live preview (fixed width)
                    ui.allocate_ui_with_layout(
                        egui::vec2(320.0, CONTENT_HEIGHT),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.heading("Live Preview");
                            ui.add_space(5.0);

                            egui::ScrollArea::vertical()
                                .id_salt("theme_editor_preview_scroll")
                                .max_height(CONTENT_HEIGHT - 30.0)
                                .show(ui, |ui| {
                                    Self::show_theme_preview(&self.theme_editor_theme, ui);
                                });
                        },
                    );
                });

                ui.add_space(10.0);
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("✓ Apply & Close").clicked() {
                        // Save the custom theme to the list
                        let new_theme = self.theme_editor_theme.clone();
                        let theme_name = new_theme.name.clone();

                        // Check if a theme with this name already exists
                        if let Some(existing) = self
                            .user_settings
                            .custom_themes
                            .iter_mut()
                            .find(|t| t.name == theme_name)
                        {
                            // Update existing theme
                            *existing = new_theme.clone();
                        } else {
                            // Add new theme
                            self.user_settings.custom_themes.push(new_theme.clone());
                        }

                        // Set as current theme
                        self.settings_form_theme = Theme::Custom(Box::new(new_theme.clone()));
                        self.user_settings.theme = Theme::Custom(Box::new(new_theme));

                        // Save settings immediately
                        if let Err(e) = self.user_settings.save() {
                            self.message = Some((format!("Failed to save theme: {}", e), true));
                        }

                        self.show_theme_editor = false;
                    }
                    if ui.button("Cancel").clicked() {
                        // Revert to the original theme before editing
                        self.settings_form_theme = self.theme_editor_original_theme.clone();
                        self.user_settings.theme = self.theme_editor_original_theme.clone();
                        self.show_theme_editor = false;
                    }
                });
            });
    }

    /// Show background color editors
    fn show_theme_backgrounds(theme: &mut CustomTheme, ui: &mut egui::Ui) {
        ui.heading("Background Colors");
        ui.add_space(10.0);

        egui::Grid::new("theme_backgrounds_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Window Fill:");
                color_picker_widget(ui, &mut theme.window_fill);
                ui.end_row();

                ui.label("Panel Fill:");
                color_picker_widget(ui, &mut theme.panel_fill);
                ui.end_row();

                ui.label("Extreme Background:");
                ui.label("(text edit backgrounds)");
                ui.end_row();
                ui.label("");
                color_picker_widget(ui, &mut theme.extreme_bg);
                ui.end_row();

                ui.label("Faint Background:");
                ui.label("(subtle separators)");
                ui.end_row();
                ui.label("");
                color_picker_widget(ui, &mut theme.faint_bg);
                ui.end_row();
            });
    }

    /// Show text color editors
    fn show_theme_text(theme: &mut CustomTheme, ui: &mut egui::Ui) {
        ui.heading("Text Colors");
        ui.add_space(10.0);

        egui::Grid::new("theme_text_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Override Text Color:");
                let mut has_override = theme.text_color.is_some();
                if ui.checkbox(&mut has_override, "Custom text color").changed() {
                    if has_override {
                        theme.text_color = Some(ThemeColor::new(200, 200, 200));
                    } else {
                        theme.text_color = None;
                    }
                }
                ui.end_row();

                if let Some(ref mut color) = theme.text_color {
                    ui.label("");
                    color_picker_widget(ui, color);
                    ui.end_row();
                }

                ui.label("Hyperlink Color:");
                color_picker_widget(ui, &mut theme.hyperlink_color);
                ui.end_row();

                ui.label("Warning Color:");
                color_picker_widget(ui, &mut theme.warn_fg);
                ui.end_row();

                ui.label("Error Color:");
                color_picker_widget(ui, &mut theme.error_fg);
                ui.end_row();
            });
    }

    /// Show widget color editors
    fn show_theme_widgets(theme: &mut CustomTheme, ui: &mut egui::Ui) {
        ui.heading("Widget Colors");
        ui.add_space(10.0);

        ui.label("Non-Interactive Widgets:");
        ui.add_space(5.0);
        egui::Grid::new("theme_widgets_noninteractive")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Background:");
                color_picker_widget(ui, &mut theme.widget_bg);
                ui.end_row();

                ui.label("Foreground:");
                color_picker_widget(ui, &mut theme.widget_fg);
                ui.end_row();
            });

        ui.add_space(15.0);
        ui.label("Interactive Widget States:");
        ui.add_space(5.0);
        egui::Grid::new("theme_widgets_interactive")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Inactive Background:");
                color_picker_widget(ui, &mut theme.widget_inactive_bg);
                ui.end_row();

                ui.label("Hovered Background:");
                color_picker_widget(ui, &mut theme.widget_hovered_bg);
                ui.end_row();

                ui.label("Active Background:");
                color_picker_widget(ui, &mut theme.widget_active_bg);
                ui.end_row();

                ui.label("Open Background:");
                color_picker_widget(ui, &mut theme.widget_open_bg);
                ui.end_row();
            });
    }

    /// Show selection color editors
    fn show_theme_selection(theme: &mut CustomTheme, ui: &mut egui::Ui) {
        ui.heading("Selection Colors");
        ui.add_space(10.0);

        egui::Grid::new("theme_selection_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Selection Background:");
                color_picker_widget(ui, &mut theme.selection_bg);
                ui.end_row();

                ui.label("Selection Foreground:");
                color_picker_widget(ui, &mut theme.selection_fg);
                ui.end_row();
            });
    }

    /// Show border and rounding editors
    fn show_theme_borders(theme: &mut CustomTheme, ui: &mut egui::Ui) {
        ui.heading("Borders & Rounding");
        ui.add_space(10.0);

        ui.label("Border Colors:");
        ui.add_space(5.0);
        egui::Grid::new("theme_borders_colors")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Inactive Stroke:");
                color_picker_widget(ui, &mut theme.widget_stroke_color);
                ui.end_row();

                ui.label("Hovered Stroke:");
                color_picker_widget(ui, &mut theme.widget_hovered_stroke_color);
                ui.end_row();

                ui.label("Active Stroke:");
                color_picker_widget(ui, &mut theme.widget_active_stroke_color);
                ui.end_row();
            });

        ui.add_space(15.0);
        ui.label("Stroke Width & Rounding:");
        ui.add_space(5.0);
        egui::Grid::new("theme_borders_sizing")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Stroke Width:");
                ui.add(
                    egui::Slider::new(&mut theme.widget_stroke_width, 0.0..=5.0)
                        .suffix("px"),
                );
                ui.end_row();

                ui.label("Widget Rounding:");
                ui.add(
                    egui::Slider::new(&mut theme.widget_rounding, 0.0..=20.0)
                        .suffix("px"),
                );
                ui.end_row();

                ui.label("Window Rounding:");
                ui.add(
                    egui::Slider::new(&mut theme.window_rounding, 0.0..=20.0)
                        .suffix("px"),
                );
                ui.end_row();
            });

        ui.add_space(15.0);
        ui.label("Shadows:");
        ui.add_space(5.0);
        ui.checkbox(&mut theme.window_shadow, "Window Shadow");
        ui.checkbox(&mut theme.popup_shadow, "Popup Shadow");
    }

    /// Show spacing and layout editors
    fn show_theme_spacing(theme: &mut CustomTheme, ui: &mut egui::Ui) {
        ui.heading("Spacing & Layout");
        ui.add_space(10.0);

        egui::Grid::new("theme_spacing_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                ui.label("Item Spacing (H, V):");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut theme.item_spacing.0).speed(0.5));
                    ui.add(egui::DragValue::new(&mut theme.item_spacing.1).speed(0.5));
                });
                ui.end_row();

                ui.label("Button Padding (H, V):");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut theme.button_padding.0).speed(0.5));
                    ui.add(egui::DragValue::new(&mut theme.button_padding.1).speed(0.5));
                });
                ui.end_row();

                ui.label("Window Padding:");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut theme.window_padding.0).speed(0.5));
                    ui.add(egui::DragValue::new(&mut theme.window_padding.1).speed(0.5));
                });
                ui.end_row();

                ui.label("Scroll Bar Width:");
                ui.add(
                    egui::Slider::new(&mut theme.scroll_bar_width, 4.0..=20.0)
                        .suffix("px"),
                );
                ui.end_row();

                ui.label("Indent:");
                ui.add(
                    egui::Slider::new(&mut theme.indent, 8.0..=40.0)
                        .suffix("px"),
                );
                ui.end_row();
            });
    }


    /// Show a live preview of the theme with AIDA-specific UI examples
    fn show_theme_preview(theme: &CustomTheme, ui: &mut egui::Ui) {
        ui.set_min_width(280.0);

        // === Requirements List Preview ===
        ui.strong("Requirements List");
        ui.add_space(3.0);

        // Simulated requirement list items
        let list_bg = theme.extreme_bg.to_egui();
        egui::Frame::none()
            .fill(list_bg)
            .inner_margin(4.0)
            .rounding(2.0)
            .show(ui, |ui| {
                // Selected item
                let selected_rect = ui.available_rect_before_wrap();
                let selected_rect = egui::Rect::from_min_size(
                    selected_rect.min,
                    egui::vec2(ui.available_width(), 20.0),
                );
                ui.painter()
                    .rect_filled(selected_rect, 2.0, theme.selection_bg.to_egui());
                ui.horizontal(|ui| {
                    ui.colored_label(theme.selection_fg.to_egui(), "FR-001");
                    ui.colored_label(theme.selection_fg.to_egui(), "User Authentication");
                });

                // Normal items
                ui.horizontal(|ui| {
                    ui.label("FR-002");
                    ui.label("Data Export Feature");
                });
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::GRAY, "FR-003");
                    ui.colored_label(egui::Color32::GRAY, "(filtered parent)");
                });
                ui.horizontal(|ui| {
                    ui.label("NFR-001");
                    ui.label("Performance Target");
                });
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(5.0);

        // === Detail View Preview ===
        ui.strong("Detail View");
        ui.add_space(3.0);

        egui::Frame::none()
            .fill(theme.panel_fill.to_egui())
            .inner_margin(6.0)
            .rounding(4.0)
            .stroke(egui::Stroke::new(1.0, theme.widget_stroke_color.to_egui()))
            .show(ui, |ui| {
                ui.heading("FR-001: User Authentication");
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label("Status:");
                    ui.colored_label(egui::Color32::from_rgb(76, 175, 80), "Approved");
                    ui.label("Priority:");
                    ui.colored_label(egui::Color32::from_rgb(244, 67, 54), "High");
                });
                ui.add_space(3.0);
                ui.label("Users must authenticate before accessing the system.");
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(5.0);

        // === Form Preview ===
        ui.strong("Add/Edit Form");
        ui.add_space(3.0);

        egui::Grid::new("preview_form_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Title:");
                let mut title = "New Requirement".to_string();
                ui.add(egui::TextEdit::singleline(&mut title).desired_width(150.0));
                ui.end_row();

                ui.label("Type:");
                let mut type_sel = "Functional";
                egui::ComboBox::from_id_salt("preview_type")
                    .selected_text(type_sel)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut type_sel, "Functional", "Functional");
                        ui.selectable_value(&mut type_sel, "Non-Functional", "Non-Functional");
                    });
                ui.end_row();

                ui.label("Status:");
                let mut status_sel = "Draft";
                egui::ComboBox::from_id_salt("preview_status")
                    .selected_text(status_sel)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut status_sel, "Draft", "Draft");
                        ui.selectable_value(&mut status_sel, "Approved", "Approved");
                    });
                ui.end_row();
            });

        ui.add_space(5.0);
        ui.horizontal(|ui| {
            let _ = ui.button("Save");
            let _ = ui.button("Cancel");
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(5.0);

        // === Messages Preview ===
        ui.strong("Messages");
        ui.add_space(3.0);
        ui.colored_label(egui::Color32::from_rgb(76, 175, 80), "Saved successfully");
        ui.colored_label(theme.warn_fg.to_egui(), "Warning: Unsaved changes");
        ui.colored_label(theme.error_fg.to_egui(), "Error: Failed to load");
    }

    fn show_users_table(&mut self, ui: &mut egui::Ui) {
        // Collect user data to avoid borrow issues
        let users_data: Vec<(Uuid, Option<String>, String, String, String, bool)> = self
            .store
            .users
            .iter()
            .filter(|u| self.show_archived_users || !u.archived)
            .map(|u| {
                (
                    u.id,
                    u.spec_id.clone(),
                    u.name.clone(),
                    u.email.clone(),
                    u.handle.clone(),
                    u.archived,
                )
            })
            .collect();

        if users_data.is_empty() {
            ui.label("No users defined.");
            return;
        }

        egui::Grid::new("users_table")
            .num_columns(6)
            .striped(true)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                // Header
                ui.strong("ID");
                ui.strong("Name");
                ui.strong("Email");
                ui.strong("Handle");
                ui.strong("Status");
                ui.strong("Actions");
                ui.end_row();

                for (id, spec_id, name, email, handle, archived) in &users_data {
                    // Show spec_id with special styling
                    if let Some(sid) = spec_id {
                        ui.colored_label(egui::Color32::from_rgb(74, 158, 255), sid);
                    } else {
                        ui.label("-");
                    }
                    ui.label(name);
                    ui.label(email);
                    ui.label(format!("@{}", handle));
                    ui.label(if *archived { "Archived" } else { "Active" });

                    ui.horizontal(|ui| {
                        if ui.small_button("✏").on_hover_text("Edit").clicked() {
                            self.editing_user_id = Some(*id);
                            self.user_form_name = name.clone();
                            self.user_form_email = email.clone();
                            self.user_form_handle = handle.clone();
                            self.show_user_form = true;
                        }

                        let archive_label = if *archived { "Unarchive" } else { "Archive" };
                        if ui
                            .small_button(if *archived { "↩" } else { "📁" })
                            .on_hover_text(archive_label)
                            .clicked()
                        {
                            if let Some(user) = self.store.get_user_by_id_mut(id) {
                                user.archived = !user.archived;
                            }
                            self.save();
                        }

                        if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                            self.store.remove_user(id);
                            self.save();
                        }
                    });
                    ui.end_row();
                }
            });
    }

    fn add_new_user(&mut self) {
        if self.user_form_name.is_empty() {
            self.message = Some(("User name is required".to_string(), true));
            return;
        }

        // Use add_user_with_id to auto-generate $USER-XXX spec_id
        let spec_id = self.store.add_user_with_id(
            self.user_form_name.clone(),
            self.user_form_email.clone(),
            self.user_form_handle.clone(),
        );
        self.save();

        self.show_user_form = false;
        self.user_form_name.clear();
        self.user_form_email.clear();
        self.user_form_handle.clear();
        self.message = Some((format!("User {} added successfully", spec_id), false));
    }

    fn save_edited_user(&mut self) {
        if let Some(user_id) = self.editing_user_id {
            if let Some(user) = self.store.get_user_by_id_mut(&user_id) {
                user.name = self.user_form_name.clone();
                user.email = self.user_form_email.clone();
                user.handle = self.user_form_handle.clone();
            }
            self.save();

            self.show_user_form = false;
            self.editing_user_id = None;
            self.user_form_name.clear();
            self.user_form_email.clear();
            self.user_form_handle.clear();
            self.message = Some(("User updated successfully".to_string(), false));
        }
    }

    fn backup_database(&mut self) {
        use chrono::Local;

        let db_path = self.storage.path();
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");

        // Create backup filename with timestamp
        let backup_name = if let Some(stem) = db_path.file_stem() {
            let ext = db_path
                .extension()
                .map(|e| e.to_str().unwrap_or("yaml"))
                .unwrap_or("yaml");
            format!(
                "{}_{}.{}",
                stem.to_str().unwrap_or("requirements"),
                timestamp,
                ext
            )
        } else {
            format!("requirements_backup_{}.yaml", timestamp)
        };

        let backup_path = db_path
            .parent()
            .map(|p| p.join(&backup_name))
            .unwrap_or_else(|| std::path::PathBuf::from(&backup_name));

        match std::fs::copy(db_path, &backup_path) {
            Ok(_) => {
                self.message = Some((format!("Backup created: {}", backup_name), false));
            }
            Err(e) => {
                self.message = Some((format!("Backup failed: {}", e), true));
            }
        }
    }

    /// Get indices of requirements that pass the current filters (in display order)
    /// For flat view, returns in storage order. For tree views, returns in tree traversal order.
    fn get_filtered_indices(&self) -> Vec<usize> {
        match &self.perspective {
            Perspective::Flat => {
                // Flat view: simple filtered list in storage order (all treated as root)
                self.store
                    .requirements
                    .iter()
                    .enumerate()
                    .filter(|(_, req)| self.passes_filters(req, true))
                    .map(|(idx, _)| idx)
                    .collect()
            }
            _ => {
                // Tree view: traverse in display order
                let Some((outgoing_type, _)) = self.perspective.relationship_types() else {
                    return Vec::new();
                };

                let mut result = Vec::new();
                match self.perspective_direction {
                    PerspectiveDirection::TopDown => {
                        let leaves = self.find_tree_leaves(&outgoing_type);
                        for leaf_idx in leaves {
                            self.collect_tree_indices_bottom_up(
                                leaf_idx,
                                &outgoing_type,
                                &mut result,
                            );
                        }
                    }
                    PerspectiveDirection::BottomUp => {
                        let roots = self.find_tree_roots(&outgoing_type);
                        for root_idx in roots {
                            self.collect_tree_indices_top_down(
                                root_idx,
                                &outgoing_type,
                                &mut result,
                            );
                        }
                    }
                }
                result
            }
        }
    }

    /// Collect tree indices in top-down order (roots first, then children)
    fn collect_tree_indices_top_down(
        &self,
        idx: usize,
        outgoing_rel_type: &RelationshipType,
        result: &mut Vec<usize>,
    ) {
        // Don't add duplicates
        if result.contains(&idx) {
            return;
        }

        // Check if this node is collapsed
        let is_collapsed = self
            .store
            .requirements
            .get(idx)
            .map(|req| self.tree_collapsed.get(&req.id).copied().unwrap_or(false))
            .unwrap_or(false);

        result.push(idx);

        // If not collapsed, add children recursively
        if !is_collapsed {
            if let Some(req) = self.store.requirements.get(idx) {
                let children = self.get_children(&req.id, outgoing_rel_type);
                for child_idx in children {
                    self.collect_tree_indices_top_down(child_idx, outgoing_rel_type, result);
                }
            }
        }
    }

    /// Collect tree indices in bottom-up order (leaves first, then parents)
    fn collect_tree_indices_bottom_up(
        &self,
        idx: usize,
        outgoing_rel_type: &RelationshipType,
        result: &mut Vec<usize>,
    ) {
        // Don't add duplicates
        if result.contains(&idx) {
            return;
        }

        // Check if this node is collapsed
        let is_collapsed = self
            .store
            .requirements
            .get(idx)
            .map(|req| self.tree_collapsed.get(&req.id).copied().unwrap_or(false))
            .unwrap_or(false);

        result.push(idx);

        // If not collapsed, add parents recursively
        if !is_collapsed {
            if let Some(req) = self.store.requirements.get(idx) {
                let parents = self.get_parents(&req.id, outgoing_rel_type);
                for parent_idx in parents {
                    self.collect_tree_indices_bottom_up(parent_idx, outgoing_rel_type, result);
                }
            }
        }
    }

    /// Search comments recursively for a search term
    fn search_comments_recursive(&self, comments: &[Comment], search: &str) -> bool {
        for comment in comments {
            // Check comment content
            if comment.content.to_lowercase().contains(search) {
                return true;
            }
            // Check author name
            if comment.author.to_lowercase().contains(search) {
                return true;
            }
            // Recursively check replies
            if self.search_comments_recursive(&comment.replies, search) {
                return true;
            }
        }
        false
    }

    /// Check if a requirement passes the current filters
    /// `is_root` indicates whether this is a root-level requirement (true) or a child (false)
    fn passes_filters(&self, req: &Requirement, is_root: bool) -> bool {
        // Text search filter (applies to all levels)
        if !self.filter_text.is_empty() && !self.search_scope.is_none() {
            let search = self.filter_text.to_lowercase();
            let mut found = false;

            // Check title if enabled
            if self.search_scope.title && req.title.to_lowercase().contains(&search) {
                found = true;
            }

            // Check description if enabled
            if !found
                && self.search_scope.description
                && req.description.to_lowercase().contains(&search)
            {
                found = true;
            }

            // Check spec_id if enabled
            if !found && self.search_scope.spec_id {
                if let Some(ref spec_id) = req.spec_id {
                    if spec_id.to_lowercase().contains(&search) {
                        found = true;
                    }
                }
            }

            // Check comments if enabled
            if !found && self.search_scope.comments {
                found = self.search_comments_recursive(&req.comments, &search);
            }

            if !found {
                return false;
            }
        }

        // Determine which filters to use based on root vs child
        let (filter_types, filter_features, filter_prefixes, filter_statuses, filter_priorities) =
            if is_root || self.children_same_as_root {
                // Root requirements or "same as root" mode: use root filters
                (
                    &self.filter_types,
                    &self.filter_features,
                    &self.filter_prefixes,
                    &self.filter_statuses,
                    &self.filter_priorities,
                )
            } else {
                // Child requirements with separate filters
                (
                    &self.child_filter_types,
                    &self.child_filter_features,
                    &self.child_filter_prefixes,
                    &self.child_filter_statuses,
                    &self.child_filter_priorities,
                )
            };

        // Type filter (empty = show all)
        if !filter_types.is_empty() && !filter_types.contains(&req.req_type) {
            return false;
        }

        // Feature filter (empty = show all)
        if !filter_features.is_empty() && !filter_features.contains(&req.feature) {
            return false;
        }

        // Prefix filter (empty = show all)
        if !filter_prefixes.is_empty() {
            // Extract prefix from spec_id (e.g., "SEC-001" -> "SEC")
            let req_prefix = req
                .spec_id
                .as_ref()
                .and_then(|s| s.split('-').next())
                .unwrap_or("");
            if !filter_prefixes.contains(req_prefix) {
                return false;
            }
        }

        // Status filter (empty = show all)
        if !filter_statuses.is_empty() && !filter_statuses.contains(&req.status) {
            return false;
        }

        // Priority filter (empty = show all)
        if !filter_priorities.is_empty() && !filter_priorities.contains(&req.priority) {
            return false;
        }

        // Archive filter (hide archived unless show_archived is true)
        if req.archived && !self.show_archived {
            return false;
        }

        true
    }

    /// Check if a requirement or any of its descendants (via the given relationship type) matches filters
    /// This is used to determine if non-matching ancestors should be shown (greyed out)
    fn has_matching_descendant(
        &self,
        req_id: &Uuid,
        outgoing_rel_type: &RelationshipType,
        visited: &mut HashSet<Uuid>,
    ) -> bool {
        // Prevent infinite recursion
        if visited.contains(req_id) {
            return false;
        }
        visited.insert(*req_id);

        // Find all children via the outgoing relationship type
        // In Parent/Child, outgoing is "Parent" - so we find requirements that have Parent pointing to us
        // That means we are their parent, they are our children
        for req in &self.store.requirements {
            for rel in &req.relationships {
                if &rel.rel_type == outgoing_rel_type && &rel.target_id == req_id {
                    // req has a Parent relationship pointing to req_id
                    // So req is a child of req_id
                    // Check if this child matches filters
                    if self.passes_filters(req, false) {
                        return true;
                    }
                    // Recursively check this child's descendants
                    if self.has_matching_descendant(&req.id, outgoing_rel_type, visited) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Compute all ancestor requirement IDs that should be shown (greyed out) because they have matching descendants
    fn compute_ancestor_ids_to_show(&self, outgoing_rel_type: &RelationshipType) -> HashSet<Uuid> {
        let mut ancestors_to_show = HashSet::new();
        let mut visited = HashSet::new();

        // Find all requirements that match filters
        let matching_reqs: Vec<Uuid> = self
            .store
            .requirements
            .iter()
            .filter(|req| self.passes_filters(req, false)) // Check with is_root=false for general match
            .map(|req| req.id)
            .collect();

        // For each matching requirement, trace back its ancestors
        for req_id in &matching_reqs {
            self.collect_ancestors(
                req_id,
                outgoing_rel_type,
                &mut ancestors_to_show,
                &mut visited,
            );
        }

        // Remove the matching requirements themselves (they're not "ancestors to grey out")
        for req_id in &matching_reqs {
            ancestors_to_show.remove(req_id);
        }

        ancestors_to_show
    }

    /// Recursively collect all ancestors of a requirement
    fn collect_ancestors(
        &self,
        req_id: &Uuid,
        outgoing_rel_type: &RelationshipType,
        ancestors: &mut HashSet<Uuid>,
        visited: &mut HashSet<Uuid>,
    ) {
        if visited.contains(req_id) {
            return;
        }
        visited.insert(*req_id);

        // Find the requirement
        let Some(req) = self.store.requirements.iter().find(|r| &r.id == req_id) else {
            return;
        };

        // Find parents: requirements that this one points to via the outgoing relationship
        // In Parent/Child, outgoing is "Parent", so req.relationships with type Parent point to parents
        for rel in &req.relationships {
            if &rel.rel_type == outgoing_rel_type {
                // rel.target_id is a parent
                ancestors.insert(rel.target_id);
                self.collect_ancestors(&rel.target_id, outgoing_rel_type, ancestors, visited);
            }
        }
    }

    /// Get all unique feature names from requirements
    fn get_all_features(&self) -> Vec<String> {
        let mut features: Vec<String> = self
            .store
            .requirements
            .iter()
            .map(|r| r.feature.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        features.sort();
        features
    }

    /// Find root nodes for tree view (requirements that are not children of any other requirement)
    /// For Parent/Child: roots are requirements that no one has a Parent relationship pointing to
    /// If `ancestors_to_show` is provided, also include roots that are in that set (shown greyed out)
    fn find_tree_roots_with_ancestors(
        &self,
        outgoing_rel_type: &RelationshipType,
        ancestors_to_show: &HashSet<Uuid>,
    ) -> Vec<usize> {
        // Collect all requirement IDs that are targets of the outgoing relationship type
        // These are the "children" - the ones that parents point to
        let mut is_child: HashSet<Uuid> = HashSet::new();

        for req in &self.store.requirements {
            for rel in &req.relationships {
                if &rel.rel_type == outgoing_rel_type {
                    // This requirement has a Parent/outgoing relationship to target
                    // So target is a child
                    is_child.insert(rel.target_id);
                }
            }
        }

        // Return indices of requirements that are NOT children (i.e., they are roots)
        // Include if: passes filters OR is an ancestor of something that passes filters
        self.store
            .requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| {
                !is_child.contains(&req.id)
                    && (self.passes_filters(req, true) || ancestors_to_show.contains(&req.id))
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Find root nodes for tree view (requirements that are not children of any other requirement)
    /// For Parent/Child: roots are requirements that no one has a Parent relationship pointing to
    fn find_tree_roots(&self, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        self.find_tree_roots_with_ancestors(outgoing_rel_type, &HashSet::new())
    }

    /// Get children of a requirement for a given relationship type
    /// If `ancestors_to_show` is provided, also include children that are in that set (shown greyed out)
    fn get_children_with_ancestors(
        &self,
        parent_id: &Uuid,
        outgoing_rel_type: &RelationshipType,
        ancestors_to_show: &HashSet<Uuid>,
    ) -> Vec<usize> {
        // Find the parent requirement
        if let Some(parent) = self.store.requirements.iter().find(|r| &r.id == parent_id) {
            // Get all target IDs where relationship type matches
            let child_ids: Vec<Uuid> = parent
                .relationships
                .iter()
                .filter(|r| &r.rel_type == outgoing_rel_type)
                .map(|r| r.target_id)
                .collect();

            // Convert to indices
            // Include if: passes filters OR is an ancestor of something that passes filters
            self.store
                .requirements
                .iter()
                .enumerate()
                .filter(|(_, req)| {
                    child_ids.contains(&req.id)
                        && (self.passes_filters(req, false) || ancestors_to_show.contains(&req.id))
                })
                .map(|(idx, _)| idx)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get children of a requirement for a given relationship type
    fn get_children(&self, parent_id: &Uuid, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        self.get_children_with_ancestors(parent_id, outgoing_rel_type, &HashSet::new())
    }

    /// Get parents of a requirement for a given relationship type (for bottom-up view)
    /// Finds requirements that have an outgoing relationship (e.g., Parent) pointing to this child
    fn get_parents(&self, child_id: &Uuid, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        // Find all requirements that have an outgoing relationship to this child
        // e.g., find all requirements with a "Parent" relationship where target_id == child_id
        // In bottom-up view, parents are shown nested under children, so they're "children" in display terms
        self.store
            .requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| {
                self.passes_filters(req, false)
                    && req
                        .relationships
                        .iter()
                        .any(|r| &r.rel_type == outgoing_rel_type && &r.target_id == child_id)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Find leaf nodes for bottom-up tree view (requirements with no outgoing relationships of the type)
    /// These are displayed at root level in bottom-up view
    fn find_tree_leaves(&self, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        self.store
            .requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| {
                self.passes_filters(req, true)
                    && !req
                        .relationships
                        .iter()
                        .any(|r| &r.rel_type == outgoing_rel_type)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Create a relationship based on drag-drop action and current perspective
    /// Returns (source_idx, target_idx, relationship_type) where source stores the relationship to target
    fn get_relationship_for_drop(
        &self,
        dragged_idx: usize,
        drop_target_idx: usize,
    ) -> Option<(usize, usize, RelationshipType)> {
        if dragged_idx == drop_target_idx {
            return None; // Can't create relationship to self
        }

        let (outgoing_type, _incoming_type) = self.perspective.relationship_types()?;

        // In Parent/Child perspective:
        // - outgoing_type is Parent (stored on the child, pointing to the parent)
        // - When dragging in top-down: drop target becomes parent of dragged item
        //   So dragged (child) gets a Parent relationship pointing to drop_target (parent)
        // - When dragging in bottom-up: dragged item becomes parent of drop target
        //   So drop_target (child) gets a Parent relationship pointing to dragged (parent)
        match self.perspective_direction {
            PerspectiveDirection::TopDown => {
                // Drop target becomes parent of dragged
                // Dragged (child) stores Parent relationship pointing to drop_target (parent)
                Some((dragged_idx, drop_target_idx, outgoing_type))
            }
            PerspectiveDirection::BottomUp => {
                // Dragged becomes parent of drop_target
                // drop_target (child) stores Parent relationship pointing to dragged (parent)
                Some((drop_target_idx, dragged_idx, outgoing_type))
            }
        }
    }

    /// Create a relationship between two requirements
    fn create_relationship_from_drop(&mut self, dragged_idx: usize, drop_target_idx: usize) {
        if let Some((source_idx, target_idx, rel_type)) =
            self.get_relationship_for_drop(dragged_idx, drop_target_idx)
        {
            let source_id = self.store.requirements.get(source_idx).map(|r| r.id);
            let target_id = self.store.requirements.get(target_idx).map(|r| r.id);

            if let (Some(source_id), Some(target_id)) = (source_id, target_id) {
                // Validate the relationship first
                let validation = self
                    .store
                    .validate_relationship(&source_id, &rel_type, &target_id);

                // Check for errors
                if !validation.valid {
                    let error_msg = validation.errors.join("; ");
                    self.message =
                        Some((format!("Cannot create relationship: {}", error_msg), true));
                    return;
                }

                // Show warnings but proceed
                if !validation.warnings.is_empty() {
                    // We'll show the warning along with success
                    let _warning_msg = validation.warnings.join("; ");
                }

                // Set the relationship (replaces any existing relationship of same type)
                // source stores the relationship pointing to target
                // Use inverse from definitions to determine bidirectionality
                let bidirectional = self.store.get_inverse_type(&rel_type).is_some();
                match self.store.set_relationship(
                    &source_id,
                    rel_type.clone(),
                    &target_id,
                    bidirectional,
                ) {
                    Ok(()) => {
                        self.save();
                        // Get display name from definition
                        let rel_name = self
                            .store
                            .get_definition_for_type(&rel_type)
                            .map(|d| d.display_name.clone())
                            .unwrap_or_else(|| format!("{:?}", rel_type));

                        let mut msg = format!("Relationship '{}' set", rel_name);
                        if !validation.warnings.is_empty() {
                            msg = format!("{} (Warning: {})", msg, validation.warnings.join("; "));
                        }
                        self.message = Some((msg, false));
                    }
                    Err(e) => {
                        self.message = Some((format!("Failed to set relationship: {}", e), true));
                    }
                }
            }
        }
    }

    fn show_list_panel(&mut self, ctx: &egui::Context, in_form_view: bool) {
        egui::SidePanel::left("list_panel")
            .min_width(200.0) // Allow narrower panel
            .default_width(400.0)
            .show(ctx, |ui| {
                // Header with optional collapse button
                ui.horizontal(|ui| {
                    ui.heading("Requirements");
                    if in_form_view {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("▶ Hide")
                                .on_hover_text("Hide requirements list")
                                .clicked()
                            {
                                self.left_panel_collapsed = true;
                            }
                        });
                    }
                });
                ui.separator();

                // Search bar
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter_text)
                            .hint_text("Search (case-insensitive)...")
                            .desired_width(150.0),
                    );

                    // Filter toggle button
                    let filter_active =
                        !self.filter_types.is_empty() || !self.filter_features.is_empty();
                    let filter_btn_text = if filter_active {
                        "🔽 Filters ●"
                    } else {
                        "🔽 Filters"
                    };
                    if ui.button(filter_btn_text).clicked() {
                        self.show_filter_panel = !self.show_filter_panel;
                    }
                });

                // Perspective and preset selector
                ui.horizontal(|ui| {
                    ui.label("View:");

                    // Determine what to show as selected text
                    let selected_text = if let Some(ref preset_name) = self.active_preset {
                        if self.current_view_matches_active_preset() {
                            preset_name.clone()
                        } else {
                            format!("{}*", preset_name) // Modified indicator
                        }
                    } else {
                        self.perspective.label().to_string()
                    };

                    // Clone presets for iteration
                    let presets: Vec<ViewPreset> = self.user_settings.view_presets.clone();
                    let mut preset_to_apply: Option<ViewPreset> = None;
                    let mut clear_active_preset = false;

                    egui::ComboBox::from_id_salt("perspective_combo")
                        .selected_text(&selected_text)
                        .show_ui(ui, |ui| {
                            // Built-in perspectives section
                            ui.label("Built-in Views");
                            ui.separator();

                            // Check if current view matches a built-in (for highlighting)
                            let is_flat = self.perspective == Perspective::Flat
                                && self.active_preset.is_none();
                            let is_parent_child = self.perspective == Perspective::ParentChild
                                && self.active_preset.is_none();
                            let is_verification = self.perspective == Perspective::Verification
                                && self.active_preset.is_none();
                            let is_references = self.perspective == Perspective::References
                                && self.active_preset.is_none();

                            if ui
                                .selectable_label(is_flat, Perspective::Flat.label())
                                .clicked()
                            {
                                self.perspective = Perspective::Flat;
                                clear_active_preset = true;
                            }
                            if ui
                                .selectable_label(is_parent_child, Perspective::ParentChild.label())
                                .clicked()
                            {
                                self.perspective = Perspective::ParentChild;
                                clear_active_preset = true;
                            }
                            if ui
                                .selectable_label(
                                    is_verification,
                                    Perspective::Verification.label(),
                                )
                                .clicked()
                            {
                                self.perspective = Perspective::Verification;
                                clear_active_preset = true;
                            }
                            if ui
                                .selectable_label(is_references, Perspective::References.label())
                                .clicked()
                            {
                                self.perspective = Perspective::References;
                                clear_active_preset = true;
                            }

                            // User presets section (if any exist)
                            if !presets.is_empty() {
                                ui.add_space(5.0);
                                ui.label("Saved Presets");
                                ui.separator();

                                for preset in &presets {
                                    let is_selected = self.active_preset.as_ref()
                                        == Some(&preset.name)
                                        && self.current_view_matches_active_preset();

                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(is_selected, &preset.name).clicked()
                                        {
                                            preset_to_apply = Some(preset.clone());
                                        }
                                        // Delete button (small X)
                                        if ui
                                            .small_button("✕")
                                            .on_hover_text("Delete preset")
                                            .clicked()
                                        {
                                            self.show_delete_preset_confirm =
                                                Some(preset.name.clone());
                                        }
                                    });
                                }
                            }
                        });

                    // Apply preset if one was selected
                    if let Some(preset) = preset_to_apply {
                        self.apply_preset(&preset);
                    }

                    // Clear active preset if built-in was selected
                    if clear_active_preset {
                        self.active_preset = None;
                    }

                    // Direction selector (only shown for non-flat perspectives)
                    if self.perspective != Perspective::Flat {
                        egui::ComboBox::from_id_salt("direction_combo")
                            .selected_text(self.perspective_direction.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.perspective_direction,
                                    PerspectiveDirection::TopDown,
                                    "Top-down ↓",
                                );
                                ui.selectable_value(
                                    &mut self.perspective_direction,
                                    PerspectiveDirection::BottomUp,
                                    "Bottom-up ↑",
                                );
                            });
                    }

                    // Save As button (shown when view has unsaved changes)
                    if self.has_unsaved_view() {
                        if ui
                            .button("💾 Save As...")
                            .on_hover_text("Save current view as a preset")
                            .clicked()
                        {
                            // Pre-fill with active preset name if modifying, otherwise empty
                            self.preset_name_input = self.active_preset.clone().unwrap_or_default();
                            self.show_save_preset_dialog = true;
                        }
                    }

                    // Reset button (shown when not at default)
                    if self.perspective != Perspective::Flat
                        || self.perspective_direction != PerspectiveDirection::TopDown
                        || !self.filter_types.is_empty()
                        || !self.filter_features.is_empty()
                    {
                        if ui
                            .small_button("↺")
                            .on_hover_text("Reset to default view")
                            .clicked()
                        {
                            self.reset_to_default_view();
                        }
                    }
                });

                // Collapsible filter panel
                if self.show_filter_panel {
                    ui.separator();
                    self.show_filter_controls(ui);
                }

                ui.separator();

                // Check for drag auto-scroll before showing ScrollArea
                // Compute scroll delta based on pointer position during drag
                let mut scroll_delta_to_apply = 0.0;
                if self.drag_source.is_some() {
                    if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        // We need to check against the available rect for the scroll area
                        let available_rect = ui.available_rect_before_wrap();
                        let edge_zone = 50.0; // Pixels from edge to start scrolling
                        let scroll_speed = 10.0; // Pixels per frame

                        if pointer_pos.y < available_rect.top() + edge_zone
                            && pointer_pos.y >= available_rect.top()
                        {
                            // Near top - scroll up
                            let intensity =
                                1.0 - (pointer_pos.y - available_rect.top()) / edge_zone;
                            scroll_delta_to_apply = -scroll_speed * intensity;
                        } else if pointer_pos.y > available_rect.bottom() - edge_zone
                            && pointer_pos.y <= available_rect.bottom()
                        {
                            // Near bottom - scroll down
                            let intensity =
                                1.0 - (available_rect.bottom() - pointer_pos.y) / edge_zone;
                            scroll_delta_to_apply = scroll_speed * intensity;
                        }
                    }
                }

                // Requirement list (flat or tree) with drag auto-scroll support
                // Use horizontal() for manual horizontal scrolling (no auto-scroll on selection)
                // wrapped with vertical() for vertical scrolling with auto-scroll on selection
                let mut scroll_area = egui::ScrollArea::vertical()
                    .id_salt("requirements_list_scroll")
                    .auto_shrink([false, false]); // Don't shrink to content

                // If we need to scroll due to drag, set the scroll offset
                if scroll_delta_to_apply != 0.0 {
                    let new_offset = (self.drag_scroll_delta + scroll_delta_to_apply).max(0.0);
                    self.drag_scroll_delta = new_offset;
                    scroll_area = scroll_area.vertical_scroll_offset(new_offset);
                    ui.ctx().request_repaint();
                }

                let scroll_output = scroll_area.show(ui, |ui| {
                    // Wrap content in horizontal scroll for wide titles
                    // This is separate from vertical scroll so scroll_to_me only scrolls vertically
                    egui::ScrollArea::horizontal()
                        .id_salt("requirements_list_horizontal_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Set minimum width for content to prevent text wrapping
                            ui.set_min_width(300.0);
                            match &self.perspective {
                                Perspective::Flat => {
                                    self.show_flat_list(ui);
                                }
                                _ => {
                                    self.show_tree_list(ui);
                                }
                            }
                        });
                });

                // Update stored offset from actual scroll state (for when user scrolls manually)
                if self.drag_source.is_some() {
                    self.drag_scroll_delta = scroll_output.state.offset.y;
                } else {
                    self.drag_scroll_delta = scroll_output.state.offset.y;
                }

                // Selection remains fixed when scrolling - user must click to change selection
            });
    }

    fn show_filter_controls(&mut self, ui: &mut egui::Ui) {
        // Search scope section
        ui.label("Search In:");
        ui.horizontal_wrapped(|ui| {
            // "Everything" toggle - when checked, enables all; when unchecked, clears all
            let mut everything = self.search_scope.is_all();
            if ui.checkbox(&mut everything, "Everything").changed() {
                if everything {
                    self.search_scope = SearchScope::all();
                } else {
                    // When unchecking "Everything", enable only Title as a sensible default
                    self.search_scope = SearchScope {
                        title: true,
                        description: false,
                        comments: false,
                        spec_id: false,
                    };
                }
            }

            ui.separator();

            // Individual scope checkboxes (only shown when not "Everything")
            if !self.search_scope.is_all() {
                ui.checkbox(&mut self.search_scope.title, "Title");
                ui.checkbox(&mut self.search_scope.description, "Description");
                ui.checkbox(&mut self.search_scope.comments, "Comments");
                ui.checkbox(&mut self.search_scope.spec_id, "ID");
            }
        });

        ui.separator();

        // Filter tabs: Root and Children
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.filter_tab == FilterTab::Root, "Root")
                .clicked()
            {
                self.filter_tab = FilterTab::Root;
            }
            if ui
                .selectable_label(self.filter_tab == FilterTab::Children, "Children")
                .clicked()
            {
                self.filter_tab = FilterTab::Children;
            }
        });

        ui.separator();

        match self.filter_tab {
            FilterTab::Root => {
                self.show_root_filter_controls(ui);
            }
            FilterTab::Children => {
                self.show_children_filter_controls(ui);
            }
        }

        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_archived, "Show Archived");
            ui.checkbox(&mut self.show_filtered_parents, "Show Parents")
                .on_hover_text(
                    "Show greyed-out parent requirements for filtered items in tree views",
                );
        });
    }

    fn show_root_filter_controls(&mut self, ui: &mut egui::Ui) {
        ui.label("Type Filters:");
        ui.horizontal_wrapped(|ui| {
            let types = [
                (RequirementType::Functional, "FR"),
                (RequirementType::NonFunctional, "NFR"),
                (RequirementType::System, "SR"),
                (RequirementType::User, "UR"),
                (RequirementType::ChangeRequest, "CR"),
                (RequirementType::Bug, "BUG"),
                (RequirementType::Epic, "Epic"),
                (RequirementType::Story, "Story"),
                (RequirementType::Task, "Task"),
                (RequirementType::Spike, "Spike"),
            ];

            for (req_type, label) in types {
                let mut checked = self.filter_types.contains(&req_type);
                if ui.checkbox(&mut checked, label).changed() {
                    if checked {
                        self.filter_types.insert(req_type);
                    } else {
                        self.filter_types.remove(&req_type);
                    }
                }
            }

            if ui.small_button("Clear").clicked() {
                self.filter_types.clear();
            }
        });

        ui.add_space(5.0);
        ui.label("Feature Filters:");

        let features = self.get_all_features();
        ui.horizontal_wrapped(|ui| {
            for feature in &features {
                let mut checked = self.filter_features.contains(feature);
                // Truncate long feature names for display
                let display_name = if feature.len() > 15 {
                    format!("{}...", &feature[..12])
                } else {
                    feature.clone()
                };

                if ui
                    .checkbox(&mut checked, &display_name)
                    .on_hover_text(feature)
                    .changed()
                {
                    if checked {
                        self.filter_features.insert(feature.clone());
                    } else {
                        self.filter_features.remove(feature);
                    }
                }
            }

            if ui.small_button("Clear").clicked() {
                self.filter_features.clear();
            }
        });

        // Prefix filters
        let prefixes = self.store.get_all_prefixes();
        if !prefixes.is_empty() {
            ui.add_space(5.0);
            ui.label("ID Prefix Filters:");
            ui.horizontal_wrapped(|ui| {
                for prefix in &prefixes {
                    let mut checked = self.filter_prefixes.contains(prefix);
                    if ui.checkbox(&mut checked, prefix).changed() {
                        if checked {
                            self.filter_prefixes.insert(prefix.clone());
                        } else {
                            self.filter_prefixes.remove(prefix);
                        }
                    }
                }

                if ui.small_button("Clear").clicked() {
                    self.filter_prefixes.clear();
                }
            });
        }

        // Status filters
        ui.add_space(5.0);
        ui.label("Status Filters:");
        ui.horizontal_wrapped(|ui| {
            let statuses = [
                (RequirementStatus::Draft, "Draft"),
                (RequirementStatus::Approved, "Approved"),
                (RequirementStatus::Completed, "Completed"),
                (RequirementStatus::Rejected, "Rejected"),
            ];

            for (status, label) in statuses {
                let mut checked = self.filter_statuses.contains(&status);
                if ui.checkbox(&mut checked, label).changed() {
                    if checked {
                        self.filter_statuses.insert(status);
                    } else {
                        self.filter_statuses.remove(&status);
                    }
                }
            }

            if ui.small_button("Clear").clicked() {
                self.filter_statuses.clear();
            }
        });

        // Priority filters
        ui.add_space(5.0);
        ui.label("Priority Filters:");
        ui.horizontal_wrapped(|ui| {
            let priorities = [
                (RequirementPriority::High, "High"),
                (RequirementPriority::Medium, "Medium"),
                (RequirementPriority::Low, "Low"),
            ];

            for (priority, label) in priorities {
                let mut checked = self.filter_priorities.contains(&priority);
                if ui.checkbox(&mut checked, label).changed() {
                    if checked {
                        self.filter_priorities.insert(priority);
                    } else {
                        self.filter_priorities.remove(&priority);
                    }
                }
            }

            if ui.small_button("Clear").clicked() {
                self.filter_priorities.clear();
            }
        });
    }

    fn show_children_filter_controls(&mut self, ui: &mut egui::Ui) {
        // "Same as root" checkbox
        ui.checkbox(&mut self.children_same_as_root, "Same as root");
        ui.add_space(5.0);

        // Disable/grey out the controls when "Same as root" is checked
        let enabled = !self.children_same_as_root;

        ui.add_enabled_ui(enabled, |ui| {
            ui.label("Type Filters:");
            ui.horizontal_wrapped(|ui| {
                let types = [
                    (RequirementType::Functional, "FR"),
                    (RequirementType::NonFunctional, "NFR"),
                    (RequirementType::System, "SR"),
                    (RequirementType::User, "UR"),
                    (RequirementType::ChangeRequest, "CR"),
                    (RequirementType::Bug, "BUG"),
                    (RequirementType::Epic, "Epic"),
                    (RequirementType::Story, "Story"),
                    (RequirementType::Task, "Task"),
                    (RequirementType::Spike, "Spike"),
                ];

                for (req_type, label) in types {
                    let mut checked = self.child_filter_types.contains(&req_type);
                    if ui.checkbox(&mut checked, label).changed() {
                        if checked {
                            self.child_filter_types.insert(req_type);
                        } else {
                            self.child_filter_types.remove(&req_type);
                        }
                    }
                }

                if ui.small_button("Clear").clicked() {
                    self.child_filter_types.clear();
                }
            });

            ui.add_space(5.0);
            ui.label("Feature Filters:");

            let features = self.get_all_features();
            ui.horizontal_wrapped(|ui| {
                for feature in &features {
                    let mut checked = self.child_filter_features.contains(feature);
                    // Truncate long feature names for display
                    let display_name = if feature.len() > 15 {
                        format!("{}...", &feature[..12])
                    } else {
                        feature.clone()
                    };

                    if ui
                        .checkbox(&mut checked, &display_name)
                        .on_hover_text(feature)
                        .changed()
                    {
                        if checked {
                            self.child_filter_features.insert(feature.clone());
                        } else {
                            self.child_filter_features.remove(feature);
                        }
                    }
                }

                if ui.small_button("Clear").clicked() {
                    self.child_filter_features.clear();
                }
            });

            // Prefix filters
            let prefixes = self.store.get_all_prefixes();
            if !prefixes.is_empty() {
                ui.add_space(5.0);
                ui.label("ID Prefix Filters:");
                ui.horizontal_wrapped(|ui| {
                    for prefix in &prefixes {
                        let mut checked = self.child_filter_prefixes.contains(prefix);
                        if ui.checkbox(&mut checked, prefix).changed() {
                            if checked {
                                self.child_filter_prefixes.insert(prefix.clone());
                            } else {
                                self.child_filter_prefixes.remove(prefix);
                            }
                        }
                    }

                    if ui.small_button("Clear").clicked() {
                        self.child_filter_prefixes.clear();
                    }
                });
            }

            // Status filters
            ui.add_space(5.0);
            ui.label("Status Filters:");
            ui.horizontal_wrapped(|ui| {
                let statuses = [
                    (RequirementStatus::Draft, "Draft"),
                    (RequirementStatus::Approved, "Approved"),
                    (RequirementStatus::Completed, "Completed"),
                    (RequirementStatus::Rejected, "Rejected"),
                ];

                for (status, label) in statuses {
                    let mut checked = self.child_filter_statuses.contains(&status);
                    if ui.checkbox(&mut checked, label).changed() {
                        if checked {
                            self.child_filter_statuses.insert(status);
                        } else {
                            self.child_filter_statuses.remove(&status);
                        }
                    }
                }

                if ui.small_button("Clear").clicked() {
                    self.child_filter_statuses.clear();
                }
            });

            // Priority filters
            ui.add_space(5.0);
            ui.label("Priority Filters:");
            ui.horizontal_wrapped(|ui| {
                let priorities = [
                    (RequirementPriority::High, "High"),
                    (RequirementPriority::Medium, "Medium"),
                    (RequirementPriority::Low, "Low"),
                ];

                for (priority, label) in priorities {
                    let mut checked = self.child_filter_priorities.contains(&priority);
                    if ui.checkbox(&mut checked, label).changed() {
                        if checked {
                            self.child_filter_priorities.insert(priority);
                        } else {
                            self.child_filter_priorities.remove(&priority);
                        }
                    }
                }

                if ui.small_button("Clear").clicked() {
                    self.child_filter_priorities.clear();
                }
            });
        });
    }

    fn show_flat_list(&mut self, ui: &mut egui::Ui) {
        // Collect filtered indices first to avoid borrow issues (flat view uses root filters)
        let filtered_indices: Vec<usize> = self
            .store
            .requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| self.passes_filters(req, true))
            .map(|(idx, _)| idx)
            .collect();

        for idx in filtered_indices {
            self.show_draggable_requirement(ui, idx, 0);
        }
    }

    /// Get the status icon for a requirement status string (uses user settings)
    fn get_status_icon(&self, status_string: &str) -> String {
        self.user_settings.status_icons.get_icon(status_string).to_string()
    }

    /// Check if a status is considered "inactive" (greyed out)
    fn is_inactive_status(status_string: &str) -> bool {
        let status_lower = status_string.to_lowercase();
        status_lower.contains("completed")
            || status_lower.contains("done")
            || status_lower.contains("rejected")
            || status_lower.contains("closed")
            || status_lower.contains("verified")
    }

    /// Render a single requirement item with drag-and-drop support
    fn show_draggable_requirement(&mut self, ui: &mut egui::Ui, idx: usize, indent: usize) {
        let Some(req) = self.store.requirements.get(idx) else {
            return;
        };

        let req_id = req.id;
        let spec_id = req.spec_id.clone();
        let title = req.title.clone();
        let status_string = req.effective_status();
        let is_inactive = Self::is_inactive_status(&status_string);
        let selected = self.selected_idx == Some(idx);
        let is_drag_source = self.drag_source == Some(idx);
        let is_drop_target = self.drop_target == Some(idx);
        let can_drag = self.perspective != Perspective::Flat; // Only allow drag in tree views
        let should_scroll_to = self.scroll_to_requirement == Some(req_id);
        let show_status_icons = self.user_settings.show_status_icons;

        let indent_space = indent as f32 * 20.0;

        ui.horizontal(|ui| {
            ui.add_space(indent_space);

            // Build the label with optional status icon
            let label = if show_status_icons {
                let icon = self.get_status_icon(&status_string);
                format!("{} {} - {}", icon, spec_id.as_deref().unwrap_or("N/A"), title)
            } else {
                format!("{} - {}", spec_id.as_deref().unwrap_or("N/A"), title)
            };

            // Visual feedback for drag/drop state
            let (bg_color, stroke) = if is_drop_target && can_drag {
                (
                    egui::Color32::from_rgba_unmultiplied(100, 200, 100, 60),
                    egui::Stroke::new(2.0, egui::Color32::GREEN),
                )
            } else if is_drag_source {
                (
                    egui::Color32::from_rgba_unmultiplied(100, 100, 200, 60),
                    egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE),
                )
            } else if selected {
                (ui.visuals().selection.bg_fill, egui::Stroke::NONE)
            } else {
                (egui::Color32::TRANSPARENT, egui::Stroke::NONE)
            };

            // Create an interactive area that supports both click and drag
            let sense = if can_drag {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::click()
            };

            // Calculate size for the label
            let text = egui::WidgetText::from(&label);
            let galley = text.into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            );
            let desired_size = galley.size() + egui::vec2(8.0, 4.0); // padding

            let (rect, response) = ui.allocate_exact_size(desired_size, sense);

            // Scroll to this item if requested (e.g., after adding a new requirement)
            if should_scroll_to {
                response.scroll_to_me(Some(egui::Align::Center));
                self.scroll_to_requirement = None; // Clear after scrolling
            }

            // Paint background
            if bg_color != egui::Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, 2.0, bg_color);
            }
            if stroke != egui::Stroke::NONE {
                ui.painter().rect_stroke(rect, 2.0, stroke);
            }

            // Paint text
            let text_pos = rect.min + egui::vec2(4.0, 2.0);
            let text_color = if selected {
                ui.visuals().selection.stroke.color
            } else if is_inactive {
                // Grey out completed/rejected items
                ui.visuals().weak_text_color()
            } else {
                ui.visuals().text_color()
            };
            ui.painter().galley(text_pos, galley, text_color);

            // Handle interactions
            if response.clicked() {
                self.selected_idx = Some(idx);
                self.pending_view_change = Some(View::Detail);
            }

            // Drag handling
            if can_drag {
                if response.drag_started() {
                    self.drag_source = Some(idx);
                }

                // Check if this is a drop target using pointer position (more reliable than hovered())
                if self.drag_source.is_some() && self.drag_source != Some(idx) {
                    if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        if rect.contains(pointer_pos) {
                            self.drop_target = Some(idx);
                        }
                    }
                }

                // Release is handled globally in the update loop
            }

            // Show drag indicator while dragging
            if is_drag_source && ui.input(|i| i.pointer.is_decidedly_dragging()) {
                // Show a tooltip-style indicator following the cursor
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    egui::Area::new(egui::Id::new("drag_indicator"))
                        .fixed_pos(pos + egui::vec2(10.0, 10.0))
                        .order(egui::Order::Tooltip)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.label(format!("📎 {}", spec_id.as_deref().unwrap_or("N/A")));
                            });
                        });
                }
            }
        });
    }

    fn show_tree_list(&mut self, ui: &mut egui::Ui) {
        // Get the relationship types for the current perspective
        let Some((outgoing_type, _incoming_type)) = self.perspective.relationship_types() else {
            // Fallback to flat list if no relationship types
            self.show_flat_list(ui);
            return;
        };

        // Compute ancestors that should be shown (greyed out) because they have matching descendants
        // Only compute if the "Show Parents" toggle is enabled
        let ancestors_to_show = if self.show_filtered_parents {
            self.compute_ancestor_ids_to_show(&outgoing_type)
        } else {
            HashSet::new()
        };

        match self.perspective_direction {
            PerspectiveDirection::TopDown => {
                // Find leaves (no outgoing relationships - they are not parents of anything)
                let leaves = self.find_tree_leaves(&outgoing_type);

                if leaves.is_empty() {
                    ui.label("No leaf requirements found for this perspective.");
                    ui.label("(All requirements have outgoing relationships)");
                    ui.add_space(10.0);
                    ui.label("Showing flat list instead:");
                    ui.separator();
                    self.show_flat_list(ui);
                } else {
                    for leaf_idx in leaves {
                        self.show_tree_node_bottom_up(ui, leaf_idx, &outgoing_type, 0);
                    }
                }
            }
            PerspectiveDirection::BottomUp => {
                // Find roots (requirements that are not children of anyone)
                // Include ancestors that have matching descendants (shown greyed out) if toggle is enabled
                let roots = self.find_tree_roots_with_ancestors(&outgoing_type, &ancestors_to_show);

                if roots.is_empty() {
                    ui.label("No root requirements found for this perspective.");
                    ui.label("(All requirements have incoming relationships)");
                    ui.add_space(10.0);
                    ui.label("Showing flat list instead:");
                    ui.separator();
                    self.show_flat_list(ui);
                } else {
                    for root_idx in roots {
                        self.show_tree_node_with_ancestors(
                            ui,
                            root_idx,
                            &outgoing_type,
                            0,
                            &ancestors_to_show,
                        );
                    }
                }
            }
        }
    }

    fn show_tree_node(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        outgoing_rel_type: &RelationshipType,
        depth: usize,
    ) {
        // Call the version with no ancestors set (old behavior for backward compatibility)
        self.show_tree_node_with_ancestors(ui, idx, outgoing_rel_type, depth, &HashSet::new());
    }

    fn show_tree_node_with_ancestors(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        outgoing_rel_type: &RelationshipType,
        depth: usize,
        ancestors_to_show: &HashSet<Uuid>,
    ) {
        let Some(req) = self.store.requirements.get(idx) else {
            return;
        };

        let req_id = req.id;
        // Check if this item is dimmed (shown only because it's an ancestor of a matching item)
        let is_dimmed =
            ancestors_to_show.contains(&req_id) && !self.passes_filters(req, depth == 0);

        let children =
            self.get_children_with_ancestors(&req_id, outgoing_rel_type, ancestors_to_show);
        let has_children = !children.is_empty();

        let is_collapsed = self.tree_collapsed.get(&req_id).copied().unwrap_or(false);

        // Show expand/collapse button and requirement on same line
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 20.0);

            // Expand/collapse button or placeholder of same size
            let btn_size = egui::vec2(18.0, 18.0);
            if has_children {
                let btn_text = if is_collapsed { "+" } else { "-" };
                if ui
                    .add_sized(btn_size, egui::Button::new(btn_text))
                    .clicked()
                {
                    self.tree_collapsed.insert(req_id, !is_collapsed);
                }
            } else {
                // Placeholder with same size to maintain alignment
                ui.add_space(btn_size.x + 4.0); // Button width + spacing
            }

            // Show the draggable requirement inline (dimmed if it's an ancestor-only item)
            self.show_draggable_requirement_inline(ui, idx, is_dimmed);
        });

        // Show children if expanded
        if has_children && !is_collapsed {
            for child_idx in children {
                self.show_tree_node_with_ancestors(
                    ui,
                    child_idx,
                    outgoing_rel_type,
                    depth + 1,
                    ancestors_to_show,
                );
            }
        }
    }

    /// Render requirement item inline (without indent, for use in tree nodes)
    /// If `dimmed` is true, the item is shown greyed out (ancestor shown for context)
    fn show_draggable_requirement_inline(&mut self, ui: &mut egui::Ui, idx: usize, dimmed: bool) {
        let Some(req) = self.store.requirements.get(idx) else {
            return;
        };

        let req_id = req.id;
        let spec_id = req.spec_id.clone();
        let title = req.title.clone();
        let status_string = req.effective_status();
        let is_inactive = Self::is_inactive_status(&status_string);
        let selected = self.selected_idx == Some(idx);
        let is_drag_source = self.drag_source == Some(idx);
        let is_drop_target = self.drop_target == Some(idx);
        let should_scroll_to = self.scroll_to_requirement == Some(req_id);
        let show_status_icons = self.user_settings.show_status_icons;

        // Build the label with optional status icon
        let label = if show_status_icons {
            let icon = self.get_status_icon(&status_string);
            format!("{} {} - {}", icon, spec_id.as_deref().unwrap_or("N/A"), title)
        } else {
            format!("{} - {}", spec_id.as_deref().unwrap_or("N/A"), title)
        };

        // Visual feedback for drag/drop state
        let (bg_color, stroke) = if is_drop_target {
            (
                egui::Color32::from_rgba_unmultiplied(100, 200, 100, 60),
                egui::Stroke::new(2.0, egui::Color32::GREEN),
            )
        } else if is_drag_source {
            (
                egui::Color32::from_rgba_unmultiplied(100, 100, 200, 60),
                egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE),
            )
        } else if selected {
            (ui.visuals().selection.bg_fill, egui::Stroke::NONE)
        } else {
            (egui::Color32::TRANSPARENT, egui::Stroke::NONE)
        };

        // Calculate size for the label
        let text = egui::WidgetText::from(&label);
        let galley = text.into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let desired_size = galley.size() + egui::vec2(8.0, 4.0);

        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

        // Scroll to this item if requested (e.g., after adding a new requirement)
        if should_scroll_to {
            response.scroll_to_me(Some(egui::Align::Center));
            self.scroll_to_requirement = None;
        }

        // Paint background
        if bg_color != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, 2.0, bg_color);
        }
        if stroke != egui::Stroke::NONE {
            ui.painter().rect_stroke(rect, 2.0, stroke);
        }

        // Paint text - dimmed items use a lighter/greyed color
        let text_pos = rect.min + egui::vec2(4.0, 2.0);
        let text_color = if dimmed {
            // Greyed out for non-matching ancestors
            egui::Color32::from_gray(140)
        } else if selected {
            ui.visuals().selection.stroke.color
        } else if is_inactive {
            // Grey out completed/rejected items
            ui.visuals().weak_text_color()
        } else {
            ui.visuals().text_color()
        };
        ui.painter().galley(text_pos, galley, text_color);

        // Handle interactions
        if response.double_clicked() {
            // Double-click opens for editing
            self.selected_idx = Some(idx);
            self.load_form_from_requirement(idx);
            self.pending_view_change = Some(View::Edit);
        } else if response.clicked() {
            self.selected_idx = Some(idx);
            self.pending_view_change = Some(View::Detail);
        }

        if response.drag_started() {
            self.drag_source = Some(idx);
        }

        // Check if this is a drop target using pointer position
        if self.drag_source.is_some() && self.drag_source != Some(idx) {
            if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pointer_pos) {
                    self.drop_target = Some(idx);
                }
            }
        }

        // Release is handled globally in the update loop

        // Show drag indicator while dragging
        if is_drag_source && ui.input(|i| i.pointer.is_decidedly_dragging()) {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                egui::Area::new(egui::Id::new("drag_indicator_inline"))
                    .fixed_pos(pos + egui::vec2(10.0, 10.0))
                    .order(egui::Order::Tooltip)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(format!("📎 {}", spec_id.as_deref().unwrap_or("N/A")));
                        });
                    });
            }
        }
    }

    fn show_tree_node_bottom_up(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        outgoing_rel_type: &RelationshipType,
        depth: usize,
    ) {
        let Some(req) = self.store.requirements.get(idx) else {
            return;
        };

        let req_id = req.id;
        let parents = self.get_parents(&req_id, outgoing_rel_type);
        let has_parents = !parents.is_empty();

        let is_collapsed = self.tree_collapsed.get(&req_id).copied().unwrap_or(false);

        // Show expand/collapse button and requirement on same line
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 20.0);

            // Expand/collapse button or placeholder of same size
            let btn_size = egui::vec2(18.0, 18.0);
            if has_parents {
                let btn_text = if is_collapsed { "+" } else { "-" };
                if ui
                    .add_sized(btn_size, egui::Button::new(btn_text))
                    .clicked()
                {
                    self.tree_collapsed.insert(req_id, !is_collapsed);
                }
            } else {
                // Placeholder with same size to maintain alignment
                ui.add_space(btn_size.x + 4.0); // Button width + spacing
            }

            // Show the draggable requirement inline (not dimmed for bottom-up view for now)
            self.show_draggable_requirement_inline(ui, idx, false);
        });

        // Show parents if expanded (going up the tree)
        if has_parents && !is_collapsed {
            for parent_idx in parents {
                self.show_tree_node_bottom_up(ui, parent_idx, outgoing_rel_type, depth + 1);
            }
        }
    }

    fn show_detail_view(&mut self, ui: &mut egui::Ui) {
        if let Some(idx) = self.selected_idx {
            if let Some(req) = self.store.requirements.get(idx).cloned() {
                // Buttons need mutable access, so handle them separately
                let mut load_edit = false;
                let mut delete_req = false;
                let mut toggle_archive = false;
                let is_archived = req.archived;

                // Track actions from Quick Actions menu
                let mut new_priority: Option<RequirementPriority> = None;
                let mut new_status: Option<RequirementStatus> = None;
                let current_priority = req.priority.clone();
                let current_status = req.status.clone();

                ui.horizontal(|ui| {
                    ui.heading(&req.title);
                    if is_archived {
                        ui.label("(Archived)");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✏ Edit").clicked() {
                            load_edit = true;
                        }

                        // Quick Actions dropdown menu
                        ui.menu_button("⚡ Actions", |ui| {
                            // Priority submenu
                            ui.menu_button("Priority", |ui| {
                                let priorities = [
                                    RequirementPriority::High,
                                    RequirementPriority::Medium,
                                    RequirementPriority::Low,
                                ];
                                for priority in priorities {
                                    let label = if priority == current_priority {
                                        format!("✓ {}", priority)
                                    } else {
                                        format!("  {}", priority)
                                    };
                                    if ui.button(label).clicked() {
                                        new_priority = Some(priority);
                                        ui.close_menu();
                                    }
                                }
                            });

                            // Status submenu
                            ui.menu_button("Status", |ui| {
                                let statuses = [
                                    RequirementStatus::Draft,
                                    RequirementStatus::Approved,
                                    RequirementStatus::Completed,
                                    RequirementStatus::Rejected,
                                ];
                                for status in statuses {
                                    let label = if status == current_status {
                                        format!("✓ {}", status)
                                    } else {
                                        format!("  {}", status)
                                    };
                                    if ui.button(label).clicked() {
                                        new_status = Some(status);
                                        ui.close_menu();
                                    }
                                }

                                ui.separator();

                                // Archive/Unarchive action
                                let archive_label = if is_archived {
                                    "↩ Unarchive"
                                } else {
                                    "📁 Archive"
                                };
                                if ui.button(archive_label).clicked() {
                                    toggle_archive = true;
                                    ui.close_menu();
                                }

                                // Delete action
                                if ui.button("🗑 Delete").clicked() {
                                    delete_req = true;
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                });

                if load_edit {
                    self.load_form_from_requirement(idx);
                    self.pending_view_change = Some(View::Edit);
                }
                if delete_req {
                    self.pending_delete = Some(idx);
                }
                if toggle_archive {
                    self.toggle_archive(idx);
                }

                // Apply priority change
                if let Some(priority) = new_priority {
                    if let Some(req) = self.store.requirements.get_mut(idx) {
                        let old_priority = req.priority.clone();
                        req.priority = priority.clone();
                        let change = Requirement::field_change(
                            "priority",
                            old_priority.to_string(),
                            priority.to_string(),
                        );
                        req.record_change(self.user_settings.display_name(), vec![change]);
                        self.save();
                    }
                }

                // Apply status change
                if let Some(status) = new_status {
                    if let Some(req) = self.store.requirements.get_mut(idx) {
                        let old_status = req.status.clone();
                        req.status = status.clone();
                        let change = Requirement::field_change(
                            "status",
                            old_status.to_string(),
                            status.to_string(),
                        );
                        req.record_change(self.user_settings.display_name(), vec![change]);
                        self.save();
                    }
                }

                ui.separator();

                // Metadata grid (always shown)
                egui::Grid::new("detail_grid")
                    .num_columns(2)
                    .spacing([40.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("ID:");
                        ui.label(req.spec_id.as_deref().unwrap_or("N/A"));
                        ui.end_row();

                        ui.label("Status:");
                        ui.label(format!("{:?}", req.status));
                        ui.end_row();

                        ui.label("Priority:");
                        ui.label(format!("{:?}", req.priority));
                        ui.end_row();

                        ui.label("Type:");
                        ui.label(format!("{:?}", req.req_type));
                        ui.end_row();

                        ui.label("Feature:");
                        ui.label(&req.feature);
                        ui.end_row();

                        ui.label("Owner:");
                        ui.label(&req.owner);
                        ui.end_row();

                        if !req.tags.is_empty() {
                            ui.label("Tags:");
                            let tags_vec: Vec<String> = req.tags.iter().cloned().collect();
                            ui.label(tags_vec.join(", "));
                            ui.end_row();
                        }
                    });

                ui.separator();

                // Tabbed content
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.active_tab,
                        DetailTab::Description,
                        "📄 Description",
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        DetailTab::Comments,
                        format!("💬 Comments ({})", req.comments.len()),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        DetailTab::Links,
                        format!("🔗 Links ({})", req.relationships.len() + req.urls.len()),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        DetailTab::History,
                        format!("📜 History ({})", req.history.len()),
                    );
                });

                ui.separator();

                // Tab content
                let req_id = req.id;
                egui::ScrollArea::vertical().show(ui, |ui| match &self.active_tab {
                    DetailTab::Description => {
                        self.show_description_tab(ui, &req, idx);
                    }
                    DetailTab::Comments => {
                        self.show_comments_tab(ui, &req, idx);
                    }
                    DetailTab::Links => {
                        self.show_links_tab(ui, &req, req_id);
                    }
                    DetailTab::History => {
                        self.show_history_tab(ui, &req);
                    }
                });
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading("Select a requirement from the list");
            });
        }
    }

    fn show_description_tab(&mut self, ui: &mut egui::Ui, req: &Requirement, idx: usize) {
        ui.horizontal(|ui| {
            ui.heading("Description");
            ui.label("(double-click to edit)")
                .on_hover_text("Double-click anywhere on the description to edit it");
        });
        ui.add_space(10.0);

        // Wrap description in a frame that detects double-click
        let frame_response = egui::Frame::none().show(ui, |ui| {
            // Make the frame respond to clicks
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), ui.available_height()),
                egui::Sense::click(),
            );

            // Render description as markdown inside the allocated area
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                CommonMarkViewer::new().show(ui, &mut self.markdown_cache, &req.description);
            });

            response
        });

        // Check for double-click on the description area
        if frame_response.inner.double_clicked() {
            self.load_form_from_requirement(idx);
            self.focus_description = true;
            self.pending_view_change = Some(View::Edit);
        }
    }

    fn show_comments_tab(&mut self, ui: &mut egui::Ui, req: &Requirement, idx: usize) {
        ui.horizontal(|ui| {
            ui.heading("Comments");
            if ui.button("➕ Add Comment").clicked() {
                self.show_add_comment = true;
                self.reply_to_comment = None;
                // Pre-fill author from user settings
                self.comment_author = self.user_settings.display_name();
                self.comment_content.clear();
            }
        });

        ui.add_space(10.0);

        if self.show_add_comment {
            self.show_comment_form(ui, idx);
        }

        if req.comments.is_empty() {
            ui.label("No comments yet");
        } else {
            for comment in &req.comments {
                self.show_comment_tree(ui, comment, idx, 0);
            }
        }
    }

    fn show_links_tab(&mut self, ui: &mut egui::Ui, req: &Requirement, req_id: Uuid) {
        // Show URL form modal if active
        if self.show_url_form {
            self.show_url_form_modal(ui, req_id);
            return;
        }

        // URLs section
        ui.horizontal(|ui| {
            ui.heading("External Links");
            if ui.button("➕ New URL").clicked() {
                self.editing_url_id = None;
                self.url_form_url.clear();
                self.url_form_title.clear();
                self.url_form_description.clear();
                self.url_verification_status = None;
                self.url_verification_in_progress = false;
                self.show_url_form = true;
            }
        });
        ui.add_space(5.0);

        if req.urls.is_empty() {
            ui.label("No external links");
        } else {
            let urls = req.urls.clone();
            let mut url_to_remove: Option<Uuid> = None;
            let mut url_to_edit: Option<Uuid> = None;

            for url_link in &urls {
                ui.horizontal(|ui| {
                    // Remove button
                    if ui.small_button("x").on_hover_text("Remove link").clicked() {
                        url_to_remove = Some(url_link.id);
                    }

                    // Edit button
                    if ui.small_button("✏").on_hover_text("Edit link").clicked() {
                        url_to_edit = Some(url_link.id);
                    }

                    // Verification status indicator
                    if let Some(ok) = url_link.last_verified_ok {
                        if ok {
                            ui.label("✅").on_hover_text(format!(
                                "Verified: {}",
                                url_link
                                    .last_verified
                                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                                    .unwrap_or_default()
                            ));
                        } else {
                            ui.label("❌").on_hover_text("Last verification failed");
                        }
                    }

                    // Clickable link
                    let link_text = if url_link.title.is_empty() {
                        &url_link.url
                    } else {
                        &url_link.title
                    };

                    if ui.link(link_text).on_hover_text(&url_link.url).clicked() {
                        if let Err(e) = open::that(&url_link.url) {
                            self.message = Some((format!("Failed to open URL: {}", e), true));
                        }
                    }

                    // Show description if present
                    if let Some(ref desc) = url_link.description {
                        ui.label(format!("- {}", desc));
                    }
                });
            }

            // Handle edit
            if let Some(id) = url_to_edit {
                if let Some(url_link) = urls.iter().find(|u| u.id == id) {
                    self.editing_url_id = Some(id);
                    self.url_form_url = url_link.url.clone();
                    self.url_form_title = url_link.title.clone();
                    self.url_form_description = url_link.description.clone().unwrap_or_default();
                    self.url_verification_status = None;
                    self.url_verification_in_progress = false;
                    self.show_url_form = true;
                }
            }

            // Handle remove
            if let Some(id) = url_to_remove {
                if let Some(idx) = self.selected_idx {
                    if let Some(req) = self.store.requirements.get_mut(idx) {
                        req.urls.retain(|u| u.id != id);
                        self.save();
                        self.message = Some(("URL removed".to_string(), false));
                    }
                }
            }
        }

        ui.add_space(15.0);
        ui.separator();
        ui.add_space(10.0);

        // Relationships section
        ui.horizontal(|ui| {
            ui.heading("Relationships");
            ui.add_space(20.0);
            ui.checkbox(&mut self.show_recursive_relationships, "Recursive")
                .on_hover_text(
                    "Show relationships as a tree, recursively expanding related requirements",
                );
        });
        ui.add_space(10.0);

        if req.relationships.is_empty() {
            ui.label("No relationships defined");
        } else if self.show_recursive_relationships {
            // Recursive tree view
            self.show_relationships_tree(ui, req_id, 0, &mut Vec::new(), None);
        } else {
            // Immediate relationships view (original behavior)
            self.show_immediate_relationships(ui, req_id);
        }
    }

    /// Show immediate (non-recursive) relationships for a requirement
    fn show_immediate_relationships(&mut self, ui: &mut egui::Ui, req_id: Uuid) {
        let Some(req) = self.store.requirements.iter().find(|r| r.id == req_id) else {
            return;
        };

        // Collect relationship info first to avoid borrow issues
        let rel_info: Vec<_> = req
            .relationships
            .iter()
            .map(|rel| {
                let target_idx = self
                    .store
                    .requirements
                    .iter()
                    .position(|r| r.id == rel.target_id);
                let target_label = self
                    .store
                    .requirements
                    .iter()
                    .find(|r| r.id == rel.target_id)
                    .and_then(|r| r.spec_id.as_ref())
                    .cloned()
                    .unwrap_or_else(|| "Unknown".to_string());
                let target_title = self
                    .store
                    .requirements
                    .iter()
                    .find(|r| r.id == rel.target_id)
                    .map(|r| r.title.clone())
                    .unwrap_or_else(|| "(not found)".to_string());

                // Get display name and color from relationship definition
                let (display_name, color) = self
                    .store
                    .get_definition_for_type(&rel.rel_type)
                    .map(|def| (def.display_name.clone(), def.color.clone()))
                    .unwrap_or_else(|| (format!("{}", rel.rel_type), None));

                (
                    rel.rel_type.clone(),
                    rel.target_id,
                    target_idx,
                    target_label,
                    target_title,
                    display_name,
                    color,
                )
            })
            .collect();

        let mut relationship_to_remove: Option<(RelationshipType, Uuid)> = None;

        for (rel_type, target_id, target_idx, target_label, target_title, display_name, color) in
            rel_info
        {
            ui.horizontal(|ui| {
                // Break link button
                if ui
                    .small_button("x")
                    .on_hover_text("Remove relationship")
                    .clicked()
                {
                    relationship_to_remove = Some((rel_type.clone(), target_id));
                }

                // Show color indicator if defined
                if let Some(ref hex_color) = color {
                    if let Some(c) = parse_hex_color(hex_color) {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, c);
                    }
                }

                // Use display name from definition
                let label = format!("{} {} - {}", display_name, target_label, target_title);

                let response = ui.add(egui::Label::new(&label).sense(egui::Sense::click()));

                // Show hover cursor and tooltip
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                response.clone().on_hover_text("Double-click to view");

                // Navigate on double-click
                if response.double_clicked() {
                    if let Some(idx) = target_idx {
                        self.selected_idx = Some(idx);
                        self.pending_view_change = Some(View::Detail);
                    }
                }
            });
        }

        // Remove relationship if requested
        if let Some((rel_type, target_id)) = relationship_to_remove {
            // Check if the relationship has an inverse defined
            let bidirectional = self.store.get_inverse_type(&rel_type).is_some();
            if let Err(e) =
                self.store
                    .remove_relationship(&req_id, &rel_type, &target_id, bidirectional)
            {
                self.message = Some((format!("Failed to remove relationship: {}", e), true));
            } else {
                self.save();
                self.message = Some(("Relationship removed".to_string(), false));
            }
        }
    }

    /// Show relationships as a recursive tree structure
    /// `ancestor_path` tracks the path of requirement IDs from root to current node
    /// `follow_rel_type` if Some, only show relationships of this type (for consistent traversal direction)
    fn show_relationships_tree(
        &mut self,
        ui: &mut egui::Ui,
        req_id: Uuid,
        depth: usize,
        ancestor_path: &mut Vec<Uuid>,
        follow_rel_type: Option<&RelationshipType>,
    ) {
        // Get the root requirement ID (first in path, or current if at root)
        let root_id = ancestor_path.first().copied().unwrap_or(req_id);

        let Some(req) = self.store.requirements.iter().find(|r| r.id == req_id) else {
            return;
        };

        // Filter relationships based on whether we're following a specific type
        let relationships_to_show: Vec<_> = if let Some(rel_type) = follow_rel_type {
            // When recursing, only show relationships of the same type
            req.relationships
                .iter()
                .filter(|r| &r.rel_type == rel_type)
                .collect()
        } else {
            // At root (depth 0), show all relationships
            req.relationships.iter().collect()
        };

        if relationships_to_show.is_empty() {
            if depth == 0 {
                ui.label("No relationships defined");
            }
            return;
        }

        // Collect relationship info
        let rel_info: Vec<_> = relationships_to_show
            .iter()
            .map(|rel| {
                let target_idx = self
                    .store
                    .requirements
                    .iter()
                    .position(|r| r.id == rel.target_id);
                let target_label = self
                    .store
                    .requirements
                    .iter()
                    .find(|r| r.id == rel.target_id)
                    .and_then(|r| r.spec_id.as_ref())
                    .cloned()
                    .unwrap_or_else(|| "Unknown".to_string());
                let target_title = self
                    .store
                    .requirements
                    .iter()
                    .find(|r| r.id == rel.target_id)
                    .map(|r| r.title.clone())
                    .unwrap_or_else(|| "(not found)".to_string());
                let target_req = self
                    .store
                    .requirements
                    .iter()
                    .find(|r| r.id == rel.target_id);

                // Get display name and color from relationship definition
                let (display_name, color) = self
                    .store
                    .get_definition_for_type(&rel.rel_type)
                    .map(|def| (def.display_name.clone(), def.color.clone()))
                    .unwrap_or_else(|| (format!("{}", rel.rel_type), None));

                // Check how many expandable children the target actually has
                // (relationships of same type that don't point back to ancestors or current req)
                let mut path_with_current = ancestor_path.clone();
                path_with_current.push(req_id);
                let expandable_children_count = target_req
                    .map(|t| {
                        t.relationships
                            .iter()
                            // Must be same relationship type
                            .filter(|tr| tr.rel_type == rel.rel_type)
                            // Must not point back to any ancestor
                            .filter(|tr| !path_with_current.contains(&tr.target_id))
                            .count()
                    })
                    .unwrap_or(0);

                // Check if target is shared (has multiple parents via same relationship type)
                // Count how many requirements have this target as a child
                let parent_count = self
                    .store
                    .requirements
                    .iter()
                    .filter(|r| {
                        r.relationships.iter().any(|rel2| {
                            rel2.target_id == rel.target_id && rel2.rel_type == rel.rel_type
                        })
                    })
                    .count();
                let is_shared = parent_count > 1;

                // Check for cross-relationship: target has a relationship to root via different rel type
                let cross_relationship = if rel.target_id != root_id {
                    target_req.and_then(|t| {
                        t.relationships
                            .iter()
                            .find(|tr| tr.target_id == root_id && tr.rel_type != rel.rel_type)
                            .map(|tr| tr.rel_type.clone())
                    })
                } else {
                    None
                };

                (
                    rel.rel_type.clone(),
                    rel.target_id,
                    target_idx,
                    target_label,
                    target_title,
                    display_name,
                    color,
                    expandable_children_count,
                    is_shared,
                    cross_relationship,
                )
            })
            .collect();

        let indent = depth as f32 * 20.0;

        for (
            rel_type,
            target_id,
            target_idx,
            target_label,
            target_title,
            display_name,
            color,
            expandable_children_count,
            is_shared,
            cross_relationship,
        ) in rel_info
        {
            // Skip if this would show the same requirement that's already in our path
            if ancestor_path.contains(&target_id) {
                continue;
            }

            // Check if this node is collapsed (default: collapsed)
            let collapse_key = (req_id, target_id);
            let is_collapsed = *self
                .relationship_tree_collapsed
                .get(&collapse_key)
                .unwrap_or(&true);

            // Determine if we should allow recursion and show expand button
            // expandable_children_count already accounts for same rel type and ancestor filtering
            let has_expandable_children = expandable_children_count > 0;

            ui.horizontal(|ui| {
                ui.add_space(indent);

                // Show expand/collapse button only if there are expandable children
                if has_expandable_children {
                    let icon = if is_collapsed { "▶" } else { "▼" };
                    if ui.small_button(icon).clicked() {
                        self.relationship_tree_collapsed
                            .insert(collapse_key, !is_collapsed);
                    }
                } else {
                    // Placeholder for alignment
                    ui.add_space(20.0);
                }

                // Show color indicator if defined
                if let Some(ref hex_color) = color {
                    if let Some(c) = parse_hex_color(hex_color) {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, c);
                    }
                }

                // Use display name from definition
                let label = format!("{} {} - {}", display_name, target_label, target_title);

                let response = ui.add(egui::Label::new(&label).sense(egui::Sense::click()));

                // Show hover cursor and tooltip
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                response.clone().on_hover_text("Double-click to view");

                // Navigate on double-click
                if response.double_clicked() {
                    if let Some(idx) = target_idx {
                        self.selected_idx = Some(idx);
                        self.pending_view_change = Some(View::Detail);
                    }
                }

                // Show indicators for special cases
                if is_shared {
                    let shared_response = ui.add(
                        egui::Label::new(egui::RichText::new("↑ shared").small().weak())
                            .sense(egui::Sense::click()),
                    );
                    shared_response.on_hover_text("This requirement has multiple parents");
                }

                // Show cross-relationship indicator (bidirectional via different rel type)
                if let Some(ref cross_rel_type) = cross_relationship {
                    let cross_display = self
                        .store
                        .get_definition_for_type(cross_rel_type)
                        .map(|def| def.display_name.clone())
                        .unwrap_or_else(|| format!("{}", cross_rel_type));

                    let cross_response = ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("↔ {}", cross_display))
                                .small()
                                .color(egui::Color32::LIGHT_BLUE),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if cross_response
                        .on_hover_text(format!(
                            "Also {} root. Double-click to view.",
                            cross_display.to_lowercase()
                        ))
                        .double_clicked()
                    {
                        if let Some(idx) = target_idx {
                            self.selected_idx = Some(idx);
                            self.pending_view_change = Some(View::Detail);
                        }
                    }
                }
            });

            // Recursively show children if expanded and there are expandable children
            if has_expandable_children && !is_collapsed {
                ancestor_path.push(req_id);
                self.show_relationships_tree(
                    ui,
                    target_id,
                    depth + 1,
                    ancestor_path,
                    Some(&rel_type),
                );
                ancestor_path.pop();
            }
        }
    }

    fn show_url_form_modal(&mut self, ui: &mut egui::Ui, req_id: Uuid) {
        let is_editing = self.editing_url_id.is_some();
        let title = if is_editing {
            "Edit URL Link"
        } else {
            "Add URL Link"
        };

        ui.heading(title);
        ui.add_space(10.0);

        egui::Grid::new("url_form_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("URL:");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.url_form_url)
                            .hint_text("https://example.com/...")
                            .desired_width(350.0),
                    );

                    // Verify button
                    let verify_enabled =
                        !self.url_form_url.is_empty() && !self.url_verification_in_progress;
                    if ui
                        .add_enabled(verify_enabled, egui::Button::new("🔍 Verify"))
                        .clicked()
                    {
                        self.verify_url();
                    }
                });
                ui.end_row();

                // Show verification status
                if let Some((success, ref msg)) = self.url_verification_status {
                    ui.label("");
                    if success {
                        ui.colored_label(
                            egui::Color32::from_rgb(100, 200, 100),
                            format!("✅ {}", msg),
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 100, 100),
                            format!("❌ {}", msg),
                        );
                    }
                    ui.end_row();
                }

                if self.url_verification_in_progress {
                    ui.label("");
                    ui.label("⏳ Verifying...");
                    ui.end_row();
                }

                ui.label("Title:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.url_form_title)
                        .hint_text("Display title (optional)")
                        .desired_width(350.0),
                );
                ui.end_row();

                ui.label("Description:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.url_form_description)
                        .hint_text("Brief description (optional)")
                        .desired_width(350.0),
                );
                ui.end_row();
            });

        ui.add_space(15.0);

        // Save/Cancel buttons
        ui.horizontal(|ui| {
            let can_save = !self.url_form_url.is_empty();

            if ui
                .add_enabled(can_save, egui::Button::new("💾 Save"))
                .clicked()
            {
                self.save_url_link(req_id);
            }

            if ui.button("Cancel").clicked() {
                self.show_url_form = false;
            }
        });

        if self.url_form_url.is_empty() {
            ui.small("URL is required.");
        }
    }

    fn verify_url(&mut self) {
        let url = self.url_form_url.trim();

        // Basic URL validation
        if url.is_empty() {
            self.url_verification_status = Some((false, "URL is empty".to_string()));
            return;
        }

        // Check if it looks like a valid URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.url_verification_status =
                Some((false, "URL must start with http:// or https://".to_string()));
            return;
        }

        // Try to parse URL
        match url::Url::parse(url) {
            Ok(parsed) => {
                // Perform a HEAD request to verify the URL is accessible
                // For now, just do basic validation since we don't have async HTTP in egui easily
                // In a real app, you'd use reqwest or similar

                // Check that it has a host
                if parsed.host().is_none() {
                    self.url_verification_status =
                        Some((false, "Invalid URL: no host".to_string()));
                    return;
                }

                // For now, mark as "valid format" - actual HTTP check would need async
                self.url_verification_status = Some((
                    true,
                    format!(
                        "Valid URL format (host: {})",
                        parsed.host_str().unwrap_or("unknown")
                    ),
                ));
            }
            Err(e) => {
                self.url_verification_status = Some((false, format!("Invalid URL: {}", e)));
            }
        }
    }

    fn save_url_link(&mut self, req_id: Uuid) {
        let author = if self.user_settings.name.is_empty() {
            "Unknown".to_string()
        } else {
            self.user_settings.name.clone()
        };

        if let Some(idx) = self.selected_idx {
            if let Some(req) = self.store.requirements.get_mut(idx) {
                if req.id == req_id {
                    if let Some(editing_id) = self.editing_url_id {
                        // Update existing URL
                        if let Some(url_link) = req.urls.iter_mut().find(|u| u.id == editing_id) {
                            url_link.url = self.url_form_url.clone();
                            url_link.title = self.url_form_title.clone();
                            url_link.description = if self.url_form_description.is_empty() {
                                None
                            } else {
                                Some(self.url_form_description.clone())
                            };
                            // Update verification status if we just verified
                            if let Some((success, _)) = &self.url_verification_status {
                                url_link.last_verified = Some(chrono::Utc::now());
                                url_link.last_verified_ok = Some(*success);
                            }
                        }
                    } else {
                        // Add new URL
                        let mut url_link = UrlLink::new(
                            self.url_form_url.clone(),
                            self.url_form_title.clone(),
                            author,
                        );
                        if !self.url_form_description.is_empty() {
                            url_link.description = Some(self.url_form_description.clone());
                        }
                        // Set verification status if we just verified
                        if let Some((success, _)) = &self.url_verification_status {
                            url_link.last_verified = Some(chrono::Utc::now());
                            url_link.last_verified_ok = Some(*success);
                        }
                        req.urls.push(url_link);
                    }

                    self.save();
                    self.message = Some(("URL saved".to_string(), false));
                }
            }
        }

        self.show_url_form = false;
    }

    fn show_markdown_help_modal(&mut self, ctx: &egui::Context) {
        egui::Window::new("Markdown Help")
            .collapsible(false)
            .resizable(true)
            .default_width(600.0)
            .default_height(500.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Supported Markdown Syntax");
                    ui.add_space(10.0);

                    // Headers
                    ui.group(|ui| {
                        ui.strong("Headers");
                        ui.code("# Heading 1\n## Heading 2\n### Heading 3");
                    });

                    ui.add_space(8.0);

                    // Text formatting
                    ui.group(|ui| {
                        ui.strong("Text Formatting");
                        ui.horizontal_wrapped(|ui| {
                            ui.code("**bold**");
                            ui.label("→");
                            ui.label(egui::RichText::new("bold").strong());
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.code("*italic*");
                            ui.label("→");
                            ui.label(egui::RichText::new("italic").italics());
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.code("~~strikethrough~~");
                            ui.label("→");
                            ui.label(egui::RichText::new("strikethrough").strikethrough());
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.code("`inline code`");
                            ui.label("→");
                            ui.code("inline code");
                        });
                    });

                    ui.add_space(8.0);

                    // Lists
                    ui.group(|ui| {
                        ui.strong("Lists");
                        ui.label("Unordered:");
                        ui.code("- Item 1\n- Item 2\n  - Nested item");
                        ui.add_space(4.0);
                        ui.label("Ordered:");
                        ui.code("1. First\n2. Second\n3. Third");
                    });

                    ui.add_space(8.0);

                    // Links
                    ui.group(|ui| {
                        ui.strong("Links");
                        ui.code("[Link text](https://example.com)");
                    });

                    ui.add_space(8.0);

                    // Code blocks
                    ui.group(|ui| {
                        ui.strong("Code Blocks");
                        ui.code("```\ncode block\nmultiple lines\n```");
                        ui.add_space(4.0);
                        ui.label("With language:");
                        ui.code("```rust\nfn main() {\n    println!(\"Hello\");\n}\n```");
                    });

                    ui.add_space(8.0);

                    // Blockquotes
                    ui.group(|ui| {
                        ui.strong("Blockquotes");
                        ui.code("> This is a quote\n> Multiple lines");
                    });

                    ui.add_space(8.0);

                    // Horizontal rule
                    ui.group(|ui| {
                        ui.strong("Horizontal Rule");
                        ui.code("---");
                    });

                    ui.add_space(8.0);

                    // Tables
                    ui.group(|ui| {
                        ui.strong("Tables");
                        ui.code("| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |");
                    });

                    ui.add_space(8.0);

                    // Task lists
                    ui.group(|ui| {
                        ui.strong("Task Lists");
                        ui.code("- [ ] Unchecked task\n- [x] Completed task");
                    });

                    ui.add_space(15.0);
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        self.show_markdown_help = false;
                    }
                });
            });
    }

    fn show_history_tab(&self, ui: &mut egui::Ui, req: &Requirement) {
        ui.heading("Change History");
        ui.add_space(10.0);

        if req.history.is_empty() {
            ui.label("No changes recorded yet");
        } else {
            for entry in req.history.iter().rev() {
                // Show newest first
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "🕒 {}",
                            entry.timestamp.format("%Y-%m-%d %H:%M:%S")
                        ));
                        ui.label(format!("👤 {}", entry.author));
                    });

                    ui.add_space(5.0);

                    for change in &entry.changes {
                        ui.horizontal(|ui| {
                            ui.label(format!("  📝 {}", change.field_name));
                        });
                        ui.horizontal(|ui| {
                            ui.label("    ❌");
                            ui.colored_label(
                                egui::Color32::from_rgb(200, 100, 100),
                                &change.old_value,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("    ✅");
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 200, 100),
                                &change.new_value,
                            );
                        });
                    }
                });
                ui.add_space(10.0);
            }
        }
    }

    fn show_form(&mut self, ui: &mut egui::Ui, is_edit: bool) {
        let title = if is_edit {
            "Edit Requirement"
        } else {
            "Add Requirement"
        };
        ui.heading(title);
        ui.separator();

        // Calculate available width for text fields
        let available_width = ui.available_width();

        // Title field - full width with context menu
        ui.label("Title:");
        let title_output = egui::TextEdit::singleline(&mut self.form_title)
            .desired_width(available_width)
            .show(ui);
        show_text_context_menu(
            ui,
            &title_output.response,
            &mut self.form_title,
            title_output.response.id,
            &mut self.last_text_selection,
        );
        ui.add_space(8.0);

        // Metadata row - Type first (affects available statuses), then Status, Priority
        let mut type_changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Type:");
            let old_type = self.form_type.clone();
            egui::ComboBox::new("type_combo", "")
                .selected_text(format!("{:?}", self.form_type))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.form_type,
                        RequirementType::Functional,
                        "Functional",
                    );
                    ui.selectable_value(
                        &mut self.form_type,
                        RequirementType::NonFunctional,
                        "NonFunctional",
                    );
                    ui.selectable_value(&mut self.form_type, RequirementType::System, "System");
                    ui.selectable_value(&mut self.form_type, RequirementType::User, "User");
                    ui.selectable_value(
                        &mut self.form_type,
                        RequirementType::ChangeRequest,
                        "Change Request",
                    );
                    ui.selectable_value(&mut self.form_type, RequirementType::Bug, "Bug");
                    ui.separator();
                    ui.selectable_value(&mut self.form_type, RequirementType::Epic, "Epic");
                    ui.selectable_value(&mut self.form_type, RequirementType::Story, "Story");
                    ui.selectable_value(&mut self.form_type, RequirementType::Task, "Task");
                    ui.selectable_value(&mut self.form_type, RequirementType::Spike, "Spike");
                });
            type_changed = old_type != self.form_type;

            ui.add_space(16.0);
            ui.label("Status:");
            // Get statuses for current type
            let statuses = self.store.get_statuses_for_type(&self.form_type);
            egui::ComboBox::new("status_combo", "")
                .selected_text(&self.form_status_string)
                .show_ui(ui, |ui| {
                    for status in &statuses {
                        if ui
                            .selectable_label(self.form_status_string == *status, status)
                            .clicked()
                        {
                            self.form_status_string = status.clone();
                        }
                    }
                });

            ui.add_space(16.0);
            ui.label("Priority:");
            egui::ComboBox::new("priority_combo", "")
                .selected_text(format!("{:?}", self.form_priority))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.form_priority, RequirementPriority::High, "High");
                    ui.selectable_value(
                        &mut self.form_priority,
                        RequirementPriority::Medium,
                        "Medium",
                    );
                    ui.selectable_value(&mut self.form_priority, RequirementPriority::Low, "Low");
                });
        });

        // If type changed, check if current status is valid for new type
        if type_changed {
            let statuses = self.store.get_statuses_for_type(&self.form_type);
            if !statuses.contains(&self.form_status_string) {
                // Reset to first available status for new type
                self.form_status_string = statuses
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Draft".to_string());
            }
            // Clear custom fields when type changes (they may not be relevant)
            self.form_custom_fields.clear();
        }
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            ui.label("Owner:");
            ui.add(egui::TextEdit::singleline(&mut self.form_owner).desired_width(150.0));

            ui.add_space(16.0);
            ui.label("Feature:");
            ui.add(egui::TextEdit::singleline(&mut self.form_feature).desired_width(150.0));

            ui.add_space(16.0);
            ui.label("Tags:");
            ui.add(
                egui::TextEdit::singleline(&mut self.form_tags)
                    .desired_width(200.0)
                    .hint_text("comma-separated"),
            );
        });
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            ui.label("ID Prefix:");

            if self.store.restrict_prefixes && !self.store.allowed_prefixes.is_empty() {
                // Restricted mode: show dropdown
                let current_display = if self.form_prefix.is_empty() {
                    "(default)".to_string()
                } else {
                    self.form_prefix.clone()
                };
                egui::ComboBox::new("prefix_combo", "")
                    .selected_text(&current_display)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.form_prefix.is_empty(), "(default)").clicked() {
                            self.form_prefix.clear();
                        }
                        for prefix in &self.store.allowed_prefixes.clone() {
                            if ui.selectable_label(self.form_prefix == *prefix, prefix).clicked() {
                                self.form_prefix = prefix.clone();
                            }
                        }
                    });
                ui.label("ⓘ").on_hover_text("Prefix selection restricted by project administrator");
            } else {
                // Unrestricted mode: show text input with optional dropdown
                let all_prefixes = self.store.get_all_prefixes();
                if !all_prefixes.is_empty() {
                    // Show combo box with existing prefixes + ability to type new ones
                    egui::ComboBox::new("prefix_combo", "")
                        .selected_text(if self.form_prefix.is_empty() { "(default)" } else { &self.form_prefix })
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.form_prefix.is_empty(), "(default)").clicked() {
                                self.form_prefix.clear();
                            }
                            for prefix in &all_prefixes {
                                if ui.selectable_label(self.form_prefix == *prefix, prefix).clicked() {
                                    self.form_prefix = prefix.clone();
                                }
                            }
                        });
                    ui.label("or");
                }

                ui.add(egui::TextEdit::singleline(&mut self.form_prefix)
                    .desired_width(80.0)
                    .hint_text("e.g., SEC"))
                    .on_hover_text("Optional custom prefix (A-Z only). Leave blank to use default from feature/type.");

                // Show validation status
                let prefix_trimmed = self.form_prefix.trim();
                if !prefix_trimmed.is_empty() {
                    if Requirement::validate_prefix(prefix_trimmed).is_some() {
                        ui.label("✓").on_hover_text("Valid prefix");
                    } else {
                        ui.colored_label(egui::Color32::RED, "✗")
                            .on_hover_text("Prefix must contain only uppercase letters (A-Z)");
                    }
                }
            }
        });
        ui.add_space(4.0);

        // Show parent relationship for new requirements (not edit)
        if !is_edit {
            if let Some(parent_id) = self.form_parent_id {
                let parent_info = self
                    .store
                    .requirements
                    .iter()
                    .find(|r| r.id == parent_id)
                    .map(|r| {
                        let spec = r.spec_id.as_deref().unwrap_or("N/A");
                        format!("{} - {}", spec, r.title)
                    });

                if let Some(parent_label) = parent_info {
                    ui.horizontal(|ui| {
                        ui.label("Parent:");
                        ui.label(&parent_label);
                        if ui
                            .small_button("x")
                            .on_hover_text("Remove parent")
                            .clicked()
                        {
                            self.form_parent_id = None;
                        }
                    });
                }
            }
        }

        // Show custom fields for the current type
        let custom_fields = self.store.get_custom_fields_for_type(&self.form_type);
        if !custom_fields.is_empty() {
            ui.add_space(8.0);
            ui.separator();
            ui.label("Type-specific Fields:");
            ui.add_space(4.0);

            // Sort fields by order
            let mut sorted_fields = custom_fields;
            sorted_fields.sort_by_key(|f| f.order);

            for field in sorted_fields {
                ui.horizontal(|ui| {
                    let label = if field.required {
                        format!("{}*:", field.label)
                    } else {
                        format!("{}:", field.label)
                    };
                    ui.label(&label);

                    // Get current value or default
                    let current_value = self
                        .form_custom_fields
                        .get(&field.name)
                        .cloned()
                        .or_else(|| field.default_value.clone())
                        .unwrap_or_default();

                    match field.field_type {
                        CustomFieldType::Text => {
                            let mut value = current_value;
                            if ui
                                .add(egui::TextEdit::singleline(&mut value).desired_width(200.0))
                                .changed()
                            {
                                self.form_custom_fields.insert(field.name.clone(), value);
                            }
                        }
                        CustomFieldType::TextArea => {
                            let mut value = current_value;
                            if ui
                                .add(
                                    egui::TextEdit::multiline(&mut value)
                                        .desired_width(300.0)
                                        .desired_rows(3),
                                )
                                .changed()
                            {
                                self.form_custom_fields.insert(field.name.clone(), value);
                            }
                        }
                        CustomFieldType::Select => {
                            egui::ComboBox::new(&field.name, "")
                                .selected_text(&current_value)
                                .show_ui(ui, |ui| {
                                    for option in &field.options {
                                        if ui
                                            .selectable_label(current_value == *option, option)
                                            .clicked()
                                        {
                                            self.form_custom_fields
                                                .insert(field.name.clone(), option.clone());
                                        }
                                    }
                                });
                        }
                        CustomFieldType::Boolean => {
                            let mut checked = current_value == "true";
                            if ui.checkbox(&mut checked, "").changed() {
                                self.form_custom_fields
                                    .insert(field.name.clone(), checked.to_string());
                            }
                        }
                        CustomFieldType::Number => {
                            let mut value = current_value;
                            if ui
                                .add(egui::TextEdit::singleline(&mut value).desired_width(80.0))
                                .changed()
                            {
                                // Basic numeric validation
                                if value.is_empty() || value.parse::<f64>().is_ok() {
                                    self.form_custom_fields.insert(field.name.clone(), value);
                                }
                            }
                        }
                        CustomFieldType::Date => {
                            let mut value = current_value;
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut value)
                                        .desired_width(120.0)
                                        .hint_text("YYYY-MM-DD"),
                                )
                                .changed()
                            {
                                self.form_custom_fields.insert(field.name.clone(), value);
                            }
                        }
                        CustomFieldType::User => {
                            // Show a dropdown of available users
                            let active_users: Vec<_> =
                                self.store.users.iter().filter(|u| !u.archived).collect();
                            let display_text = if current_value.is_empty() {
                                "(select user)".to_string()
                            } else {
                                active_users
                                    .iter()
                                    .find(|u| {
                                        u.id.to_string() == current_value
                                            || u.spec_id.as_deref() == Some(&current_value)
                                    })
                                    .map(|u| u.name.clone())
                                    .unwrap_or_else(|| current_value.clone())
                            };
                            egui::ComboBox::new(format!("{}_user", field.name), "")
                                .selected_text(&display_text)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(current_value.is_empty(), "(none)")
                                        .clicked()
                                    {
                                        self.form_custom_fields
                                            .insert(field.name.clone(), String::new());
                                    }
                                    for user in &active_users {
                                        let user_id = user
                                            .spec_id
                                            .clone()
                                            .unwrap_or_else(|| user.id.to_string());
                                        if ui
                                            .selectable_label(current_value == user_id, &user.name)
                                            .clicked()
                                        {
                                            self.form_custom_fields
                                                .insert(field.name.clone(), user_id);
                                        }
                                    }
                                });
                        }
                        CustomFieldType::Requirement => {
                            // Show a dropdown of requirements
                            let display_name = if current_value.is_empty() {
                                "(select requirement)".to_string()
                            } else {
                                self.store
                                    .requirements
                                    .iter()
                                    .find(|r| {
                                        r.id.to_string() == current_value
                                            || r.spec_id.as_deref() == Some(&current_value)
                                    })
                                    .map(|r| {
                                        let spec = r.spec_id.as_deref().unwrap_or("N/A");
                                        format!("{} - {}", spec, r.title)
                                    })
                                    .unwrap_or_else(|| current_value.clone())
                            };
                            egui::ComboBox::new(format!("{}_req", field.name), "")
                                .selected_text(&display_name)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(current_value.is_empty(), "(none)")
                                        .clicked()
                                    {
                                        self.form_custom_fields
                                            .insert(field.name.clone(), String::new());
                                    }
                                    for req in self.store.requirements.iter().take(50) {
                                        // Limit to prevent huge lists
                                        let req_id = req
                                            .spec_id
                                            .clone()
                                            .unwrap_or_else(|| req.id.to_string());
                                        let label = format!(
                                            "{} - {}",
                                            req.spec_id.as_deref().unwrap_or("N/A"),
                                            req.title
                                        );
                                        if ui
                                            .selectable_label(current_value == req_id, label)
                                            .clicked()
                                        {
                                            self.form_custom_fields
                                                .insert(field.name.clone(), req_id);
                                        }
                                    }
                                });
                        }
                    }

                    // Show description tooltip if available
                    if let Some(desc) = &field.description {
                        ui.label("ⓘ").on_hover_text(desc);
                    }
                });
            }
        }

        ui.add_space(8.0);

        // Description field - full width and takes remaining height
        ui.horizontal(|ui| {
            ui.label("Description:");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Preview toggle button
                let preview_label = if self.show_description_preview {
                    "✏ Edit"
                } else {
                    "👁 Preview"
                };
                if ui.button(preview_label).clicked() {
                    self.show_description_preview = !self.show_description_preview;
                }
                if ui
                    .link("Supports Markdown")
                    .on_hover_text("Click for Markdown help")
                    .clicked()
                {
                    self.show_markdown_help = true;
                }
            });
        });

        // Calculate remaining height for description (leave space for buttons)
        let remaining_height = ui.available_height() - 50.0;
        let description_height = remaining_height.max(8.0 * self.current_font_size * 1.4); // At least 8 lines

        egui::ScrollArea::vertical()
            .max_height(description_height)
            .show(ui, |ui| {
                if self.show_description_preview {
                    // Preview mode - render as markdown
                    CommonMarkViewer::new().show(
                        ui,
                        &mut self.markdown_cache,
                        &self.form_description,
                    );
                } else {
                    // Edit mode - text editor with context menu
                    let output = egui::TextEdit::multiline(&mut self.form_description)
                        .desired_width(available_width)
                        .desired_rows(8)
                        .hint_text("Enter requirement description (Markdown supported)...")
                        .show(ui);

                    // Request focus if we came here via double-click on description
                    if self.focus_description {
                        output.response.request_focus();
                        self.focus_description = false;
                    }

                    show_text_context_menu(
                        ui,
                        &output.response,
                        &mut self.form_description,
                        output.response.id,
                        &mut self.last_text_selection,
                    );
                }
            });

        ui.add_space(8.0);
        ui.separator();

        // Check for pending save (triggered by Ctrl+S keybinding)
        let should_save = self.pending_save;
        if should_save {
            self.pending_save = false;
        }

        // Check for ESC key to cancel (only if confirmation dialog is not open)
        let esc_pressed =
            !self.show_cancel_confirm_dialog && ui.input(|i| i.key_pressed(egui::Key::Escape));

        ui.horizontal(|ui| {
            if ui.button("💾 Save").clicked() || should_save {
                if is_edit {
                    if let Some(idx) = self.selected_idx {
                        self.update_requirement(idx);
                    }
                } else {
                    self.add_requirement();
                }
            }
            if ui.button("❌ Cancel").clicked() || esc_pressed {
                self.request_form_cancel(is_edit);
            }
        });

        // Show cancel confirmation dialog if there are unsaved changes
        if self.show_cancel_confirm_dialog {
            // ESC while dialog is open should close the dialog (continue editing)
            let dialog_esc = ui.input(|i| i.key_pressed(egui::Key::Escape));

            egui::Window::new("Unsaved Changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label("You have unsaved changes. Are you sure you want to cancel?");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Discard Changes").clicked() {
                            self.cancel_form(is_edit);
                        }
                        if ui.button("Continue Editing").clicked() || dialog_esc {
                            self.show_cancel_confirm_dialog = false;
                        }
                    });
                });
        }
    }

    fn show_comment_form(&mut self, ui: &mut egui::Ui, _req_idx: usize) {
        let available_width = ui.available_width();

        ui.group(|ui| {
            ui.label(if self.reply_to_comment.is_some() {
                "Add Reply"
            } else {
                "Add Comment"
            });

            ui.horizontal(|ui| {
                ui.label("Author:");
                ui.add(egui::TextEdit::singleline(&mut self.comment_author).desired_width(200.0));
            });

            ui.label("Content:");
            let comment_output = egui::TextEdit::multiline(&mut self.comment_content)
                .desired_width(available_width - 20.0) // Account for group padding
                .desired_rows(4)
                .hint_text("Enter comment...")
                .show(ui);
            show_text_context_menu(
                ui,
                &comment_output.response,
                &mut self.comment_content,
                comment_output.response.id,
                &mut self.last_text_selection,
            );

            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    if !self.comment_author.is_empty() && !self.comment_content.is_empty() {
                        self.pending_comment_add = Some((
                            self.comment_author.clone(),
                            self.comment_content.clone(),
                            self.reply_to_comment,
                        ));
                        self.show_add_comment = false;
                    }
                }
                if ui.button("❌ Cancel").clicked() {
                    self.show_add_comment = false;
                    self.reply_to_comment = None;
                    self.comment_author.clear();
                    self.comment_content.clear();
                }
            });
        });
    }

    fn show_comment_tree(
        &mut self,
        ui: &mut egui::Ui,
        comment: &Comment,
        req_idx: usize,
        depth: usize,
    ) {
        let indent = depth as f32 * 24.0;
        let is_collapsed = self
            .collapsed_comments
            .get(&comment.id)
            .copied()
            .unwrap_or(false);
        let comment_id = comment.id;
        let show_picker = self.show_reaction_picker == Some(comment_id);

        // Calculate available width for the comment (account for indent)
        let available_width = ui.available_width() - indent - 20.0; // 20.0 for group padding

        // Get reaction counts for display
        let reaction_counts = comment.reaction_counts();
        let current_user = self.user_settings.display_name();

        // Build list of reactions the current user has
        let user_reactions: Vec<String> = comment
            .reactions
            .iter()
            .filter(|r| r.author == current_user)
            .map(|r| r.reaction.clone())
            .collect();

        // Get reaction definitions for display
        let reaction_defs: Vec<_> = self.store.reaction_definitions.clone();

        ui.horizontal(|ui| {
            // Add horizontal indentation
            if indent > 0.0 {
                ui.add_space(indent);
            }

            ui.vertical(|ui| {
                ui.set_max_width(available_width);

                ui.group(|ui| {
                    ui.set_max_width(available_width - 16.0); // Account for group border

                    ui.horizontal(|ui| {
                        // Collapse/expand button if there are replies
                        let btn_size = egui::vec2(18.0, 18.0);
                        if !comment.replies.is_empty() {
                            let button_text = if is_collapsed { "+" } else { "-" };
                            if ui
                                .add_sized(btn_size, egui::Button::new(button_text))
                                .clicked()
                            {
                                self.collapsed_comments.insert(comment.id, !is_collapsed);
                            }
                        } else {
                            ui.add_space(btn_size.x + 4.0); // Spacing when no collapse button
                        }

                        ui.label(format!("👤 {}", comment.author));
                        ui.label(format!(
                            "🕒 {}",
                            comment.created_at.format("%Y-%m-%d %H:%M")
                        ));
                    });

                    // Comment content on its own line with text wrapping
                    ui.add(egui::Label::new(&comment.content).wrap());

                    // Display existing reactions
                    if !reaction_counts.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for def in &reaction_defs {
                                if let Some(&count) = reaction_counts.get(&def.name) {
                                    let has_reacted = user_reactions.contains(&def.name);
                                    let label = if has_reacted {
                                        format!("{} {} ✓", def.emoji, count)
                                    } else {
                                        format!("{} {}", def.emoji, count)
                                    };
                                    let btn = egui::Button::new(&label).small();
                                    let response = ui.add(btn).on_hover_text(&def.label);
                                    if response.clicked() {
                                        self.pending_reaction_toggle =
                                            Some((comment_id, def.name.clone()));
                                    }
                                }
                            }
                        });
                    }

                    ui.horizontal(|ui| {
                        if ui.small_button("💬 Reply").clicked() {
                            self.show_add_comment = true;
                            self.reply_to_comment = Some(comment.id);
                            // Pre-fill author from user settings
                            self.comment_author = self.user_settings.display_name();
                            self.comment_content.clear();
                        }

                        // Reaction picker button
                        let react_btn = if show_picker { "😊 ▼" } else { "😊" };
                        if ui
                            .small_button(react_btn)
                            .on_hover_text("Add reaction")
                            .clicked()
                        {
                            if show_picker {
                                self.show_reaction_picker = None;
                            } else {
                                self.show_reaction_picker = Some(comment_id);
                            }
                        }

                        if ui.small_button("🗑 Delete").clicked() {
                            self.pending_comment_delete = Some(comment.id);
                        }
                    });

                    // Show reaction picker if open
                    if show_picker {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("React:");
                            for def in &reaction_defs {
                                let has_reacted = user_reactions.contains(&def.name);
                                let btn_text = if has_reacted {
                                    format!("{} ✓", def.emoji)
                                } else {
                                    def.emoji.clone()
                                };
                                let response = ui.button(&btn_text).on_hover_text(&def.label);
                                if response.clicked() {
                                    self.pending_reaction_toggle =
                                        Some((comment_id, def.name.clone()));
                                    self.show_reaction_picker = None;
                                }
                            }
                        });
                    }
                });
            });
        });

        // Show replies if not collapsed
        if !is_collapsed {
            for reply in &comment.replies {
                self.show_comment_tree(ui, reply, req_idx, depth + 1);
            }
        }
    }

    fn add_comment_to_requirement(
        &mut self,
        idx: usize,
        author: String,
        content: String,
        parent_id: Option<Uuid>,
    ) {
        if let Some(req) = self.store.requirements.get_mut(idx) {
            if let Some(parent) = parent_id {
                // This is a reply
                let reply = Comment::new_reply(author, content, parent);
                if let Err(e) = req.add_reply(parent, reply) {
                    self.message = Some((format!("Error adding reply: {}", e), true));
                    return;
                }
            } else {
                // This is a top-level comment
                let comment = Comment::new(author, content);
                req.add_comment(comment);
            }

            self.save();
            self.comment_author.clear();
            self.comment_content.clear();
            self.reply_to_comment = None;
            self.message = Some(("Comment added successfully".to_string(), false));
        }
    }

    fn delete_comment_from_requirement(&mut self, idx: usize, comment_id: Uuid) {
        if let Some(req) = self.store.requirements.get_mut(idx) {
            if let Err(e) = req.delete_comment(&comment_id) {
                self.message = Some((format!("Error deleting comment: {}", e), true));
                return;
            }
            self.save();
            self.message = Some(("Comment deleted successfully".to_string(), false));
        }
    }

    fn toggle_comment_reaction(&mut self, req_idx: usize, comment_id: Uuid, reaction: &str) {
        let author = self.user_settings.display_name();
        if let Some(req) = self.store.requirements.get_mut(req_idx) {
            // Helper function to toggle reaction on a comment or its nested replies
            fn toggle_in_comments(
                comments: &mut [Comment],
                comment_id: Uuid,
                reaction: &str,
                author: &str,
            ) -> bool {
                for comment in comments.iter_mut() {
                    if comment.id == comment_id {
                        comment.toggle_reaction(reaction, author);
                        return true;
                    }
                    if toggle_in_comments(&mut comment.replies, comment_id, reaction, author) {
                        return true;
                    }
                }
                false
            }

            if toggle_in_comments(&mut req.comments, comment_id, reaction, &author) {
                self.save();
            }
        }
    }

    fn show_migration_confirmation_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_migration_dialog {
            return;
        }

        let Some((new_format, new_numbering, new_digits)) = self.pending_migration.clone() else {
            self.show_migration_dialog = false;
            return;
        };

        let validation =
            self.store
                .validate_id_config_change(&new_format, &new_numbering, new_digits);

        egui::Window::new("⚠ Confirm Migration")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.heading("Migrate Requirement IDs?");
                ui.add_space(10.0);

                ui.label(format!(
                    "This will update {} requirement ID(s) to the new format.",
                    validation.affected_count
                ));
                ui.add_space(5.0);

                ui.label("New format settings:");
                ui.indent("migration_details", |ui| {
                    ui.label(format!("• Format: {}", match new_format {
                        IdFormat::SingleLevel => "Single Level (PREFIX-NNN)",
                        IdFormat::TwoLevel => "Two Level (FEATURE-TYPE-NNN)",
                    }));
                    ui.label(format!("• Numbering: {}", match new_numbering {
                        NumberingStrategy::Global => "Global Sequential",
                        NumberingStrategy::PerPrefix => "Per Prefix",
                        NumberingStrategy::PerFeatureType => "Per Feature+Type",
                    }));
                    ui.label(format!("• Digits: {}", new_digits));
                });

                ui.add_space(10.0);
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "This action cannot be undone. Make sure to backup your requirements file first."
                );

                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    if ui.button("✅ Migrate").clicked() {
                        // Perform the migration
                        let migrated = self.store.migrate_ids_to_config(
                            new_format,
                            new_numbering,
                            new_digits,
                        );

                        // Update form fields to reflect new state
                        self.settings_form_id_format = self.store.id_config.format.clone();
                        self.settings_form_numbering = self.store.id_config.numbering.clone();
                        self.settings_form_digits = self.store.id_config.digits;

                        // Save to file
                        match self.storage.save(&self.store) {
                            Ok(()) => {
                                self.message = Some((
                                    format!("Successfully migrated {} requirement ID(s)", migrated),
                                    false
                                ));
                            }
                            Err(e) => {
                                self.message = Some((
                                    format!("Migration completed but failed to save: {}", e),
                                    true
                                ));
                            }
                        }

                        self.show_migration_dialog = false;
                        self.pending_migration = None;
                    }

                    if ui.button("❌ Cancel").clicked() {
                        self.show_migration_dialog = false;
                        self.pending_migration = None;
                    }
                });
            });
    }

    fn show_save_preset_dialog_window(&mut self, ctx: &egui::Context) {
        if !self.show_save_preset_dialog {
            return;
        }

        egui::Window::new("💾 Save View Preset")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(300.0);

                ui.label("Enter a name for this view preset:");
                ui.add_space(5.0);

                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.preset_name_input)
                        .hint_text("Preset name")
                        .desired_width(280.0),
                );

                // Focus the text field when dialog opens
                if response.gained_focus() || self.preset_name_input.is_empty() {
                    response.request_focus();
                }

                // Check if name already exists
                let name_exists = self
                    .user_settings
                    .view_presets
                    .iter()
                    .any(|p| p.name == self.preset_name_input);

                if name_exists {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "⚠ This will overwrite existing preset",
                    );
                }

                ui.add_space(10.0);

                // Show current view settings summary
                ui.group(|ui| {
                    ui.label("Current View Settings:");
                    ui.label(format!("  Perspective: {}", self.perspective.label()));
                    if self.perspective != Perspective::Flat {
                        ui.label(format!(
                            "  Direction: {}",
                            self.perspective_direction.label()
                        ));
                    }
                    if !self.filter_types.is_empty() {
                        let types: Vec<_> = self
                            .filter_types
                            .iter()
                            .map(|t| format!("{:?}", t))
                            .collect();
                        ui.label(format!("  Type filters: {}", types.join(", ")));
                    }
                    if !self.filter_features.is_empty() {
                        let features: Vec<_> = self.filter_features.iter().cloned().collect();
                        ui.label(format!("  Feature filters: {}", features.join(", ")));
                    }
                });

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    let can_save = !self.preset_name_input.trim().is_empty();

                    if ui
                        .add_enabled(can_save, egui::Button::new("Save"))
                        .clicked()
                        || (can_save && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        let name = self.preset_name_input.trim().to_string();
                        self.save_current_view_as_preset(name);
                        self.show_save_preset_dialog = false;
                        self.preset_name_input.clear();
                    }

                    if ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        self.show_save_preset_dialog = false;
                        self.preset_name_input.clear();
                    }
                });
            });
    }

    fn show_delete_preset_confirmation_dialog(&mut self, ctx: &egui::Context) {
        let preset_name = match &self.show_delete_preset_confirm {
            Some(name) => name.clone(),
            None => return,
        };

        egui::Window::new("⚠ Delete Preset")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(300.0);

                ui.label(format!("Delete preset \"{}\"?", preset_name));
                ui.add_space(5.0);
                ui.label("This action cannot be undone.");

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        self.delete_preset(&preset_name);
                        self.show_delete_preset_confirm = None;
                    }

                    if ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        self.show_delete_preset_confirm = None;
                    }
                });
            });
    }
}

impl eframe::App for RequirementsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update window title based on database name and title
        // Format: "Name - Title" or just "Title" or just "Name" or "Requirements Manager"
        let title = match (self.store.name.is_empty(), self.store.title.is_empty()) {
            (true, true) => "Requirements Manager".to_string(),
            (true, false) => self.store.title.clone(),
            (false, true) => self.store.name.clone(),
            (false, false) => format!("{} - {}", self.store.name, self.store.title),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Apply the selected theme
        self.user_settings.theme.apply(ctx);

        // Apply current font size to the context with distinct heading sizes
        let mut style = (*ctx.style()).clone();
        let base = self.current_font_size;

        // Exponential scaling for markdown headings - makes differences much more visible
        let scale = 1.25_f32;
        let heading_sizes = [
            base * scale.powi(5), // H1 = ~3.05x
            base * scale.powi(4), // H2 = ~2.44x
            base * scale.powi(3), // H3 = ~1.95x
            base * scale.powi(2), // H4 = ~1.56x
            base * scale,         // H5 = 1.25x
            base,                 // H6 = base size
        ];

        // Get the UI heading size based on user preference (1-6 maps to index 0-5)
        let ui_heading_idx = (self.user_settings.ui_heading_level.clamp(1, 6) - 1) as usize;
        let ui_heading_size = heading_sizes[ui_heading_idx];

        // Update standard text styles
        for (text_style, font_id) in style.text_styles.iter_mut() {
            match text_style {
                egui::TextStyle::Small => font_id.size = base * 0.85,
                egui::TextStyle::Body => font_id.size = base,
                egui::TextStyle::Monospace => font_id.size = base,
                egui::TextStyle::Button => font_id.size = base,
                egui::TextStyle::Heading => font_id.size = ui_heading_size, // UI headings use user preference
                egui::TextStyle::Name(name) => {
                    // Set distinct sizes for markdown heading levels (exponential scaling)
                    let name_str: &str = name.as_ref();
                    match name_str {
                        "Heading" => font_id.size = heading_sizes[0],  // H1
                        "Heading2" => font_id.size = heading_sizes[1], // H2
                        "Heading3" => font_id.size = heading_sizes[2], // H3
                        "Heading4" => font_id.size = heading_sizes[3], // H4
                        "Heading5" => font_id.size = heading_sizes[4], // H5
                        "Heading6" => font_id.size = heading_sizes[5], // H6
                        _ => font_id.size = base,
                    }
                }
            }
        }
        ctx.set_style(style);

        // Determine current keybinding context based on view state
        // If a text field has focus, we don't want to trigger navigation keys
        // Also consider if we're in Add/Edit mode - navigation shouldn't work there
        let text_input_focused = ctx.wants_keyboard_input();
        let in_form_view = matches!(self.current_view, View::Add | View::Edit);
        let in_settings = self.show_settings_dialog;

        self.current_key_context = if text_input_focused || in_form_view || in_settings {
            // When typing in a text field, in form view, or in settings - only global shortcuts should work
            KeyContext::Form // Use Form context - this is NOT Global and NOT RequirementsList
        } else {
            match self.current_view {
                View::List => KeyContext::RequirementsList,
                View::Detail => KeyContext::DetailView,
                View::Add | View::Edit => KeyContext::Form, // This branch won't be reached due to in_form_view check
            }
        };

        // Handle keyboard shortcuts for zoom (global context)
        let mut zoom_delta: f32 = 0.0;
        let mut zoom_reset = false;

        // Check for zoom keybindings
        if self.user_settings.keybindings.is_pressed(
            KeyAction::ZoomIn,
            ctx,
            self.current_key_context,
        ) {
            zoom_delta = 1.0;
        }
        if self.user_settings.keybindings.is_pressed(
            KeyAction::ZoomOut,
            ctx,
            self.current_key_context,
        ) {
            zoom_delta = -1.0;
        }
        if self.user_settings.keybindings.is_pressed(
            KeyAction::ZoomReset,
            ctx,
            self.current_key_context,
        ) {
            zoom_reset = true;
        }

        // Check for theme cycling keybinding (global context)
        if self.user_settings.keybindings.is_pressed(
            KeyAction::CycleTheme,
            ctx,
            self.current_key_context,
        ) {
            self.cycle_theme();
            self.user_settings.theme.apply(ctx);
            let _ = self.user_settings.save();
        }

        // Check for new requirement keybinding (Ctrl+N, global context)
        if self.user_settings.keybindings.is_pressed(
            KeyAction::NewRequirement,
            ctx,
            self.current_key_context,
        ) && !self.show_settings_dialog
        {
            // Switch to Add view with no parent (creates orphan requirement)
            self.form_parent_id = None;
            self.pending_view_change = Some(View::Add);
        }

        // Also handle Ctrl+= as alternate zoom in (common on keyboards)
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            let shift = i.modifiers.shift;
            if ctrl && shift && i.key_pressed(egui::Key::Equals) {
                zoom_delta = 1.0;
            }

            // Ctrl+MouseWheel to zoom - use raw scroll delta and check for events
            if ctrl {
                // Check for scroll events
                for event in &i.events {
                    if let egui::Event::MouseWheel { delta, .. } = event {
                        if delta.y > 0.0 {
                            zoom_delta = 1.0;
                        } else if delta.y < 0.0 {
                            zoom_delta = -1.0;
                        }
                    }
                }
                // Also check raw scroll delta as fallback
                if zoom_delta == 0.0 && i.raw_scroll_delta.y != 0.0 {
                    if i.raw_scroll_delta.y > 0.0 {
                        zoom_delta = 1.0;
                    } else {
                        zoom_delta = -1.0;
                    }
                }
            }
        });

        if zoom_reset {
            self.reset_zoom();
        }

        // Apply zoom after input closure
        if zoom_delta > 0.0 {
            self.zoom_in();
        } else if zoom_delta < 0.0 {
            self.zoom_out();
        }

        // Handle keyboard navigation in the requirements list
        // Context checking is now handled by the keybinding system
        {
            let mut nav_delta: i32 = 0;
            let mut jump_to_start = false;
            let mut jump_to_end = false;
            let page_size: i32 = 10; // Number of items to move for Page Up/Down

            // Check if we're in a context where list navigation should work
            let nav_context_active = matches!(
                self.current_key_context,
                KeyContext::RequirementsList | KeyContext::DetailView
            );

            // Check navigation keybindings (context-aware)
            if self.user_settings.keybindings.is_pressed(
                KeyAction::NavigateDown,
                ctx,
                self.current_key_context,
            ) {
                nav_delta = 1;
            } else if self.user_settings.keybindings.is_pressed(
                KeyAction::NavigateUp,
                ctx,
                self.current_key_context,
            ) {
                nav_delta = -1;
            }

            // Page Up/Down, Home/End, and Mouse Wheel (only when not in text input)
            if nav_context_active {
                ctx.input(|i| {
                    // Page Up/Down
                    if i.key_pressed(egui::Key::PageDown) {
                        nav_delta = page_size;
                    } else if i.key_pressed(egui::Key::PageUp) {
                        nav_delta = -page_size;
                    }

                    // Home/End
                    if i.key_pressed(egui::Key::Home) {
                        jump_to_start = true;
                    } else if i.key_pressed(egui::Key::End) {
                        jump_to_end = true;
                    }

                    // Mouse wheel scrolls the view without changing selection
                    // (Ctrl+wheel is handled separately for zoom)
                });
            }

            // Edit keybinding (context-aware)
            if self.user_settings.keybindings.is_pressed(
                KeyAction::Edit,
                ctx,
                self.current_key_context,
            ) {
                if let Some(idx) = self.selected_idx {
                    self.load_form_from_requirement(idx);
                    self.pending_view_change = Some(View::Edit);
                }
            }

            // Toggle expand/collapse in tree views (context-aware)
            if self.user_settings.keybindings.is_pressed(
                KeyAction::ToggleExpand,
                ctx,
                self.current_key_context,
            ) {
                if self.perspective != Perspective::Flat {
                    if let Some(idx) = self.selected_idx {
                        if let Some(req) = self.store.requirements.get(idx) {
                            let req_id = req.id;
                            let is_collapsed =
                                self.tree_collapsed.get(&req_id).copied().unwrap_or(false);
                            self.tree_collapsed.insert(req_id, !is_collapsed);
                        }
                    }
                }
            }

            // Save keybinding (context-aware - works in Form context)
            if self.user_settings.keybindings.is_pressed(
                KeyAction::Save,
                ctx,
                self.current_key_context,
            ) {
                self.pending_save = true;
            }

            // Handle navigation (delta-based or jump-based)
            let filtered_indices = self.get_filtered_indices();
            if !filtered_indices.is_empty() {
                let new_selection = if jump_to_start {
                    // Jump to first item
                    Some(filtered_indices[0])
                } else if jump_to_end {
                    // Jump to last item
                    Some(filtered_indices[filtered_indices.len() - 1])
                } else if nav_delta != 0 {
                    if let Some(current_idx) = self.selected_idx {
                        // Find current position in filtered list
                        if let Some(pos) =
                            filtered_indices.iter().position(|&idx| idx == current_idx)
                        {
                            // Move up or down within bounds
                            let new_pos = (pos as i32 + nav_delta)
                                .max(0)
                                .min(filtered_indices.len() as i32 - 1)
                                as usize;
                            Some(filtered_indices[new_pos])
                        } else {
                            // Current selection not in filtered list, select first/last
                            if nav_delta > 0 {
                                Some(filtered_indices[0])
                            } else {
                                Some(filtered_indices[filtered_indices.len() - 1])
                            }
                        }
                    } else {
                        // Nothing selected, select first or last based on direction
                        if nav_delta > 0 {
                            Some(filtered_indices[0])
                        } else {
                            Some(filtered_indices[filtered_indices.len() - 1])
                        }
                    }
                } else {
                    None // No navigation action
                };

                if let Some(new_sel) = new_selection {
                    if Some(new_sel) != self.selected_idx {
                        self.selected_idx = Some(new_sel);
                        self.pending_view_change = Some(View::Detail);
                        // Scroll the newly selected item into view
                        if let Some(req) = self.store.requirements.get(new_sel) {
                            self.scroll_to_requirement = Some(req.id);
                        }
                    }
                }
            }
        }

        // Handle pending operations (to avoid borrow checker issues)
        if let Some(idx) = self.pending_delete.take() {
            self.delete_requirement(idx);
        }
        if let Some(view) = self.pending_view_change.take() {
            self.current_view = view;
        }
        if let Some((author, content, parent_id)) = self.pending_comment_add.take() {
            if let Some(idx) = self.selected_idx {
                self.add_comment_to_requirement(idx, author, content, parent_id);
            }
        }
        if let Some(comment_id) = self.pending_comment_delete.take() {
            if let Some(idx) = self.selected_idx {
                self.delete_comment_from_requirement(idx, comment_id);
            }
        }
        if let Some((comment_id, reaction_name)) = self.pending_reaction_toggle.take() {
            if let Some(idx) = self.selected_idx {
                self.toggle_comment_reaction(idx, comment_id, &reaction_name);
            }
        }
        if let Some((source_idx, target_idx)) = self.pending_relationship.take() {
            self.create_relationship_from_drop(source_idx, target_idx);
        }

        // Handle drag release globally - check if primary button was just released
        let released = ctx.input(|i| i.pointer.primary_released());
        if released && self.drag_source.is_some() {
            if let (Some(source), Some(target)) = (self.drag_source, self.drop_target) {
                if source != target {
                    self.pending_relationship = Some((source, target));
                }
            }
            self.drag_source = None;
            self.drop_target = None;
        }

        // Clear drag state if mouse is not pressed (safety cleanup)
        ctx.input(|i| {
            if !i.pointer.any_down() && self.drag_source.is_some() {
                // Mouse released but we didn't catch it - clear state
            }
        });

        self.show_top_panel(ctx);

        // Determine if we should show the left panel
        // In List/Detail view: always show
        // In Add/Edit view: show if window is wide enough AND not manually collapsed
        let screen_width = ctx.screen_rect().width();
        let min_width_for_side_panel = 900.0; // Minimum width to show side panel in edit mode
        let in_form_view = self.current_view == View::Add || self.current_view == View::Edit;

        let show_left_panel = if in_form_view {
            screen_width >= min_width_for_side_panel && !self.left_panel_collapsed
        } else {
            true // Always show in List/Detail view
        };

        if show_left_panel {
            self.show_list_panel(ctx, in_form_view);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Show panel toggle button in form view when panel is hidden
            if in_form_view && !show_left_panel {
                ui.horizontal(|ui| {
                    if ui
                        .button("◀ Show List")
                        .on_hover_text("Show requirements list")
                        .clicked()
                    {
                        self.left_panel_collapsed = false;
                    }
                });
                ui.separator();
            }

            match &self.current_view {
                View::List | View::Detail => {
                    self.show_detail_view(ui);
                }
                View::Add => {
                    self.show_form(ui, false);
                }
                View::Edit => {
                    self.show_form(ui, true);
                }
            }
        });

        // Show settings dialog (modal overlay)
        self.show_settings_dialog(ctx);

        // Show theme editor dialog
        self.show_theme_editor_dialog(ctx);

        // Show icon editor dialog
        self.show_icon_editor_dialog(ctx);

        // Show project dialogs
        self.show_switch_project_dialog(ctx);
        self.show_new_project_dialog(ctx);

        // Show migration confirmation dialog
        self.show_migration_confirmation_dialog(ctx);

        // Show save preset dialog
        self.show_save_preset_dialog_window(ctx);

        // Show delete preset confirmation dialog
        self.show_delete_preset_confirmation_dialog(ctx);

        // Show markdown help modal
        if self.show_markdown_help {
            self.show_markdown_help_modal(ctx);
        }
    }
}

/// Parse a hex color string (e.g., "#ff6b6b" or "ff6b6b") into an egui Color32
fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(egui::Color32::from_rgb(r, g, b))
}
