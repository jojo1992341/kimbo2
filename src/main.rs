// Pas de console en release sous Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod logic;

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, load::SizedTexture, pos2, vec2, Align, Color32, ColorImage, CornerRadius, FontId, Frame,
    Image, Layout, Margin, Rect, RichText, Sense, Shape, Stroke, TextureHandle, TextureOptions, Ui,
};
use eframe::egui_wgpu::{SurfaceConfig, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
use eframe::wgpu;

use logic::{compute, Color, Kind, Pref, State};

const ACCENT: Color32 = Color32::from_rgb(201, 98, 43);
const SWATCH_LIGHT: Color32 = Color32::from_rgb(223, 232, 168);
const SWATCH_DARK: Color32 = Color32::from_rgb(127, 154, 74);

// ---------------------------------------------------------------------------
// Mémoire : rend à Windows les pages inutilisées (EmptyWorkingSet)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn trim_memory() {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        // « EmptyWorkingSet » de psapi.h est un alias de K32EmptyWorkingSet (kernel32).
        fn K32EmptyWorkingSet(process: *mut c_void) -> i32;
    }
    unsafe {
        K32EmptyWorkingSet(GetCurrentProcess());
    }
}

#[cfg(not(windows))]
fn trim_memory() {}

// ---------------------------------------------------------------------------
// Configuration GPU : DirectX 12 d'abord, repli automatique sur OpenGL
// ---------------------------------------------------------------------------

/// Choisit l'adaptateur : DX12 en priorité, puis OpenGL ; matériel avant logiciel ;
/// GPU intégré avant GPU dédié (économie d'énergie).
fn select_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    use wgpu::{Backend, DeviceType};

    let rank = |t: DeviceType| match t {
        DeviceType::IntegratedGpu => 0,
        DeviceType::DiscreteGpu => 1,
        DeviceType::VirtualGpu => 2,
        DeviceType::Other => 3,
        DeviceType::Cpu => 4,
    };

    for (backend, allow_cpu) in [
        (Backend::Dx12, false),
        (Backend::Gl, false),
        (Backend::Dx12, true),
        (Backend::Gl, true),
    ] {
        let mut candidates: Vec<&wgpu::Adapter> = adapters
            .iter()
            .filter(|a| {
                let info = a.get_info();
                info.backend == backend
                    && (allow_cpu || info.device_type != DeviceType::Cpu)
                    && surface.map_or(true, |s| a.is_surface_supported(s))
            })
            .collect();
        candidates.sort_by_key(|a| rank(a.get_info().device_type));
        if let Some(a) = candidates.first() {
            return Ok((*a).clone());
        }
    }
    Err("Aucun adaptateur DirectX 12 ou OpenGL compatible".to_owned())
}

fn wgpu_config() -> WgpuConfiguration {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::DX12 | wgpu::Backends::GL;
    setup.power_preference = wgpu::PowerPreference::LowPower;
    setup.native_adapter_selector = Some(Arc::new(select_adapter));
    setup.device_descriptor = Arc::new(|adapter| {
        let base_limits = if adapter.get_info().backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::default()
        };
        wgpu::DeviceDescriptor {
            label: Some("kimbo wgpu device"),
            required_limits: wgpu::Limits { max_texture_dimension_2d: 4096, ..base_limits },
            // Mémoire plutôt que performance.
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }
    });

    WgpuConfiguration {
        surface: SurfaceConfig {
            present_mode: wgpu::PresentMode::AutoVsync,
            // Moins de buffers d'affichage.
            desired_maximum_frame_latency: Some(1),
        },
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Interface
// ---------------------------------------------------------------------------

struct Textures {
    icon: TextureHandle,
    terre: TextureHandle,
    eau: TextureHandle,
    air: TextureHandle,
    feu: TextureHandle,
    pousser: TextureHandle,
}

fn load_tex(ctx: &egui::Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let img = image::load_from_memory(bytes).expect("image invalide").into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    ctx.load_texture(name, color, TextureOptions::LINEAR)
}

enum Ico<'a> {
    Swatch(Color),
    Tex(&'a TextureHandle),
}

fn swatch_color(c: Color) -> Color32 {
    match c {
        Color::Light => SWATCH_LIGHT,
        Color::Dark => SWATCH_DARK,
    }
}

fn paint_diamond(painter: &egui::Painter, center: egui::Pos2, half: f32, c: Color) {
    painter.add(Shape::convex_polygon(
        vec![
            pos2(center.x, center.y - half),
            pos2(center.x + half, center.y),
            pos2(center.x, center.y + half),
            pos2(center.x - half, center.y),
        ],
        swatch_color(c),
        Stroke::new(1.0, Color32::from_black_alpha(110)),
    ));
}

/// Bouton « segmenté » avec icônes (losange de couleur ou image) et texte.
fn seg_button(ui: &mut Ui, icons: &[Ico], text: &str, selected: bool) -> egui::Response {
    let text_col = if selected { Color32::WHITE } else { ui.visuals().text_color() };
    let galley = (!text.is_empty()).then(|| {
        ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(13.5), text_col)
    });

    let (isz, pad, gap) = (16.0_f32, 9.0_f32, 4.0_f32);
    let mut w = pad * 2.0;
    if !icons.is_empty() {
        w += icons.len() as f32 * isz + (icons.len() - 1) as f32 * gap;
    }
    if let Some(g) = &galley {
        w += g.size().x + if icons.is_empty() { 0.0 } else { gap };
    }

    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(44.0), 26.0), Sense::click());
    let fill = if selected {
        ACCENT
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(13), fill);

    let mut x = rect.left() + pad;
    let cy = rect.center().y;
    for ico in icons {
        let c = pos2(x + isz / 2.0, cy);
        match ico {
            Ico::Swatch(col) => paint_diamond(painter, c, isz / 2.0 - 1.0, *col),
            Ico::Tex(t) => {
                painter.image(
                    t.id(),
                    Rect::from_center_size(c, vec2(isz, isz)),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        x += isz + gap;
    }
    if let Some(g) = galley {
        painter.galley(pos2(x, cy - g.size().y / 2.0), g, text_col);
    }
    resp
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Ok,
    Ko,
    Warn,
}

fn tone_colors(dark: bool, tone: Tone) -> (Color32, Color32) {
    // (fond, texte)
    match (dark, tone) {
        (false, Tone::Ok) => (Color32::from_rgb(227, 243, 233), Color32::from_rgb(46, 125, 79)),
        (false, Tone::Ko) => (Color32::from_rgb(251, 230, 225), Color32::from_rgb(179, 54, 31)),
        (false, Tone::Warn) => (Color32::from_rgb(255, 241, 210), Color32::from_rgb(138, 90, 0)),
        (true, Tone::Ok) => (Color32::from_rgb(23, 48, 31), Color32::from_rgb(111, 209, 149)),
        (true, Tone::Ko) => (Color32::from_rgb(58, 28, 22), Color32::from_rgb(255, 143, 122)),
        (true, Tone::Warn) => (Color32::from_rgb(56, 48, 15), Color32::from_rgb(242, 195, 92)),
    }
}

fn tag(ui: &mut Ui, tone: Tone, text: &str) {
    let (bg, fg) = tone_colors(ui.visuals().dark_mode, tone);
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(fg).strong().size(12.5));
        });
}

fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn small_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).small().weak().strong());
}

fn inline_swatch(ui: &mut Ui, c: Color, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint_diamond(ui.painter(), rect.center(), size / 2.0 - 1.0, c);
}

fn tex_img(t: &TextureHandle, size: f32) -> Image<'static> {
    Image::new(SizedTexture::new(t.id(), vec2(size, size)))
}

struct KimboApp {
    st: State,
    tex: Textures,
    start: Instant,
    startup_trimmed: bool,
    was_focused: bool,
}

impl KimboApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let tex = Textures {
            icon: load_tex(ctx, "icon", include_bytes!("../assets/icon.png")),
            terre: load_tex(ctx, "terre", include_bytes!("../assets/terre.png")),
            eau: load_tex(ctx, "eau", include_bytes!("../assets/eau.png")),
            air: load_tex(ctx, "air", include_bytes!("../assets/air.png")),
            feu: load_tex(ctx, "feu", include_bytes!("../assets/feu.png")),
            pousser: load_tex(ctx, "pousser", include_bytes!("../assets/pousser.png")),
        };
        ctx.global_style_mut(|s| {
            s.spacing.item_spacing = vec2(6.0, 5.0);
        });
        Self {
            st: State::default(),
            tex,
            start: Instant::now(),
            startup_trimmed: false,
            was_focused: true,
        }
    }

    /// Purge mémoire : 1,5 s après le démarrage, puis à chaque perte de focus.
    fn memory_housekeeping(&mut self, ctx: &egui::Context) {
        const STARTUP_DELAY: Duration = Duration::from_millis(1500);
        if !self.startup_trimmed {
            let elapsed = self.start.elapsed();
            if elapsed >= STARTUP_DELAY {
                trim_memory();
                self.startup_trimmed = true;
            } else {
                ctx.request_repaint_after(STARTUP_DELAY - elapsed);
            }
        }
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(self.was_focused);
        if self.was_focused && !focused {
            trim_memory();
        }
        self.was_focused = focused;
    }

    fn color_row(ui: &mut Ui, label: &str, sub: &str, value: &mut Option<Color>) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(label).strong().size(14.5));
                ui.label(RichText::new(sub).small().weak());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for (c, name) in [(Color::Dark, "Foncée"), (Color::Light, "Claire")] {
                    if seg_button(ui, &[Ico::Swatch(c)], name, *value == Some(c)).clicked() {
                        *value = Some(c);
                    }
                }
            });
        });
    }

    fn inputs_ui(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            Self::color_row(ui, "Mon personnage", "Ma case actuelle", &mut self.st.me);
            ui.separator();
            Self::color_row(ui, "Disciple", "Case du Disciple du Kimbo", &mut self.st.dis);
            ui.separator();
            Self::color_row(ui, "Kimbo", "Case du boss", &mut self.st.kim);
        });

        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new("Duo d’éléments").strong().size(14.5));
                    ui.label(RichText::new("Auto = le mieux placé").small().weak());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let t = &self.tex;
                    let pair = Pref::Fixed(Kind::Pair);
                    let impair = Pref::Fixed(Kind::Impair);
                    if seg_button(ui, &[Ico::Tex(&t.air), Ico::Tex(&t.feu)], "", self.st.pref == pair)
                        .on_hover_text("Air / Feu (Glyphe pair)")
                        .clicked()
                    {
                        self.st.pref = pair;
                    }
                    if seg_button(
                        ui,
                        &[Ico::Tex(&t.terre), Ico::Tex(&t.eau)],
                        "",
                        self.st.pref == impair,
                    )
                    .on_hover_text("Terre / Eau (Glyphe impair)")
                    .clicked()
                    {
                        self.st.pref = impair;
                    }
                    if seg_button(ui, &[], "Auto", self.st.pref == Pref::Auto).clicked() {
                        self.st.pref = Pref::Auto;
                    }
                });
            });
            ui.separator();
            ui.checkbox(
                &mut self.st.statue,
                RichText::new("Succès « Statue »").strong().size(14.5),
            );
            ui.label(
                RichText::new("Mon personnage finit son tour sur sa case de départ.").small().weak(),
            );
        });
    }

    fn result_ui(&self, ui: &mut Ui) {
        let Some(p) = compute(&self.st) else {
            card(ui, |ui| {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(
                            "Choisissez la couleur de case de votre personnage, du Disciple et du Kimbo pour obtenir le placement.",
                        )
                        .weak(),
                    );
                });
                ui.add_space(24.0);
            });
            return;
        };
        let st = &self.st;
        let kim = st.kim.unwrap();
        let t = &self.tex;
        let impair = p.kind == Kind::Impair;
        let kind_name = if impair { "impair" } else { "pair" };

        // 1 · Éléments
        card(ui, |ui| {
            small_title(ui, "1 · Tapez le Kimbo en");
            let (a, an, b, bn) = if impair {
                (&t.terre, "Terre", &t.eau, "Eau")
            } else {
                (&t.air, "Air", &t.feu, "Feu")
            };
            ui.horizontal(|ui| {
                ui.add(tex_img(a, 28.0));
                ui.label(RichText::new(an).strong().size(21.0));
                ui.label(RichText::new("ou").weak());
                ui.add(tex_img(b, 28.0));
                ui.label(RichText::new(bn).strong().size(21.0));
            });
            ui.label(
                RichText::new(format!(
                    "État Glyphe {kind_name} : le Disciple pose les glyphes à distance {kind_name} de lui."
                ))
                .small()
                .weak(),
            );
        });

        // 2 · Placement
        card(ui, |ui| {
            small_title(
                ui,
                if st.statue { "2 · Restez sur votre case" } else { "2 · Placez-vous sur une case" },
            );
            ui.horizontal(|ui| {
                inline_swatch(ui, p.safe, 24.0);
                ui.label(RichText::new(format!("Case {}", p.safe.name())).strong().size(21.0));
            });
            if st.statue {
                tag(ui, Tone::Ok, "Finissez sur votre case de départ : elle est sûre avec ce duo");
            } else if p.me_placed {
                tag(ui, Tone::Ok, "Vous êtes déjà bien placé");
            } else {
                ui.horizontal(|ui| {
                    ui.add(tex_img(&t.pousser, 16.0));
                    tag(ui, Tone::Warn, "Déplacez-vous d’un nombre impair de PM (1 PM suffit)");
                });
            }
            let dark = ui.visuals().dark_mode;
            let (_, red) = tone_colors(dark, Tone::Ko);
            let mut danger = format!(
                "Case mortelle : ne finissez pas votre tour sur une case {}.",
                p.glyph.name()
            );
            if !impair {
                danger.push_str(" Évitez le corps à corps du Disciple.");
            }
            ui.label(RichText::new(danger).color(red).small().strong());
        });

        // 3 · Glyphes et Kimbo
        card(ui, |ui| {
            small_title(ui, "3 · Glyphes et Kimbo");
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Glyphes sur les cases").small().weak());
                inline_swatch(ui, p.glyph, 13.0);
                ui.label(RichText::new(p.glyph.name()).small().strong());
                ui.label(RichText::new("· Kimbo sur").small().weak());
                inline_swatch(ui, kim, 13.0);
                ui.label(RichText::new(kim.name()).small().strong());
            });

            if p.push_disciple_first {
                ui.horizontal(|ui| {
                    ui.add(tex_img(&t.pousser, 16.0));
                    tag(ui, Tone::Warn, "Poussez d’abord le Disciple d’un nombre impair de cases");
                });
            }

            if p.kim_ok {
                tag(ui, Tone::Ok, "Le Kimbo est sur la bonne couleur");
                ui.label(
                    RichText::new("Au tour suivant, il doit commencer son tour sur une case glyphée.")
                        .small()
                        .weak(),
                );
            } else {
                tag(
                    ui,
                    Tone::Ko,
                    &format!("Kimbo sur {} : hors glyphe avec ce duo", kim.name()),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.add(tex_img(&t.pousser, 16.0));
                    ui.label(RichText::new("Poussez le Kimbo d’un nombre impair de cases").strong());
                });
                if let (false, Some(alt)) = (st.statue, p.alt_safe_if_disciple_pushed) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("ou poussez le Disciple d’un nombre impair de cases (case sûre :")
                                .small(),
                        );
                        inline_swatch(ui, alt, 13.0);
                        ui.label(RichText::new(format!("{})", alt.name())).small().strong());
                    });
                }
                if st.pref != Pref::Auto {
                    ui.label(RichText::new("Ou repassez le duo d’éléments en Auto.").small().weak());
                }
            }
        });

        if st.statue {
            let (bg, fg) = tone_colors(ui.visuals().dark_mode, Tone::Warn);
            Frame::new()
                .fill(bg)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(
                            "Statue : finissez sur votre case de départ (un aller-retour est possible). Le Kimbo doit être sur la couleur opposée à la vôtre.",
                        )
                        .color(fg)
                        .small()
                        .strong(),
                    );
                });
        }
    }
}

impl eframe::App for KimboApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.memory_housekeeping(&ctx);

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(tex_img(&self.tex.icon, 30.0));
                ui.label(RichText::new("Kimbo – Placement").strong().size(19.0));
            });
            self.inputs_ui(ui);
            self.result_ui(ui);
        });
    }
}

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("icon.png invalide");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kimbo – Placement")
            .with_inner_size([420.0, 650.0])
            .with_resizable(false)
            .with_maximize_button(false)
            // Toujours au-dessus des autres fenêtres.
            .with_always_on_top()
            .with_icon(icon),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_config(),
        ..Default::default()
    };

    eframe::run_native(
        "Kimbo – Placement",
        options,
        Box::new(|cc| Ok(Box::new(KimboApp::new(cc)))),
    )
}
