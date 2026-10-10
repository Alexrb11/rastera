use eframe::egui::{self, Color32, FontId, RichText, Stroke};

pub const BACKGROUND: Color32 = Color32::from_rgb(24, 26, 29);
pub const SURFACE: Color32 = Color32::from_rgb(32, 35, 39);
pub const BORDER: Color32 = Color32::from_rgb(54, 59, 65);
pub const TEXT: Color32 = Color32::from_rgb(229, 232, 237);
pub const MUTED: Color32 = Color32::from_rgb(156, 164, 175);
pub const ACCENT: Color32 = Color32::from_rgb(116, 177, 246);
pub const SELECTED: Color32 = Color32::from_rgb(38, 61, 86);
pub const CANVAS: Color32 = Color32::from_rgb(19, 21, 24);

pub fn display_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
}

pub fn configure(ctx: &egui::Context) {
    // Use the installed Windows UI font; other platforms retain egui's fallback.
    #[cfg(target_os = "windows")]
    if let Ok(bytes) = std::fs::read(
        std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()))
            .join("Fonts/segoeui.ttf"),
    ) {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("system-ui".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "system-ui".into());
        ctx.set_fonts(fonts);
    }
    ctx.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.faint_bg_color = SURFACE;
    visuals.weak_text_color = Some(MUTED);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = 4.into();
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
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size = egui::vec2(28.0, 28.0);
        style.spacing.window_margin = 16.into();
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
        .fill(Color32::from_rgb(45, 91, 144))
        .stroke(Stroke::new(1.0, Color32::from_rgb(61, 116, 178)))
}

pub fn tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let response = ui.add(egui::Button::new(label).selected(selected).frame(false));
    if selected {
        ui.painter().line_segment(
            [response.rect.left_bottom(), response.rect.right_bottom()],
            Stroke::new(2.0, ACCENT),
        );
    }
    response
}

pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(SURFACE)
        .inner_margin(egui::Margin::symmetric(12, 8))
}

pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(MUTED));
}
