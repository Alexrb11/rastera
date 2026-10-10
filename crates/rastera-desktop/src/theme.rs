use eframe::egui::{self, Color32, FontId, RichText, Stroke};

pub const BACKGROUND: Color32 = Color32::from_rgb(29, 30, 32);
pub const SURFACE: Color32 = Color32::from_rgb(38, 40, 43);
pub const BORDER: Color32 = Color32::from_rgb(66, 69, 74);
pub const TEXT: Color32 = Color32::from_rgb(224, 225, 227);
pub const MUTED: Color32 = Color32::from_rgb(166, 169, 175);
pub const ACCENT: Color32 = Color32::from_rgb(141, 183, 236);
pub const SELECTED: Color32 = Color32::from_rgb(48, 67, 89);

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.faint_bg_color = SURFACE;
    visuals.weak_text_color = Some(MUTED);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = 3.into();
    visuals.selection.bg_fill = SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
    ] {
        widget.bg_fill = SURFACE;
        widget.weak_bg_fill = SURFACE;
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
        widget.corner_radius = 2.into();
    }
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 52, 57);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(49, 52, 57);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.active.bg_fill = SELECTED;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    ctx.set_visuals(visuals);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size = egui::vec2(26.0, 26.0);
        style.spacing.window_margin = 12.into();
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(11.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(18.0));
    });
}

pub fn primary(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.into()).color(TEXT))
        .fill(SELECTED)
        .stroke(Stroke::new(1.0, BORDER))
}

pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(MUTED));
}
