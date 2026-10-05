//! Rocket League's boost meter (the HUD gauge in the bottom-right corner), engine-agnostic.
//!
//! Presentation only. The meter is a Scaleform movie clip in the game's HUD (`GFX_Hud_SF`,
//! symbol `BoostMeterViewMovie`); its artwork (textures, the 101 frames of the fill, the text fields
//! and fonts) is extracted from the installed game by `tools/rl_assets/hud.py`. This module is the
//! clip's ActionScript, `tagame.hud.BoostMeterView` (with `FrameBasedProgressBar` and the parts of
//! `HudScreen` / `UIScreen` that place it), ported by hand:
//!
//! * the displayed number eases to each new boost value in 0.2 s and drives both fill timelines
//!   (`frame = int(boost / 100 * 101)`);
//! * above 80 the meter switches to its "max" look (pink/red tint, red glow, wider text glow, the
//!   blinking "BOOST" label), and back to amber at 80 or below;
//! * gaining boost flashes the fill white and pops the glow to twice its size, settling in 0.8 s;
//! * the clip is tilted in 3D (`rotationX = -2`, `rotationY = 15`, fill and number 20 px in front)
//!   and seen through its own perspective projection (focal length 500, centre 50 px left of and
//!   100 px above the meter): [`BoostMeterLayout::project`].
//!
//! Tweens follow GreenSock TweenMax as the HUD uses it (time based; default ease `Quad.easeOut`,
//! `Strong.easeOut` = quintic). The "team coloured boost meter" option, sounds
//! and the tutorial highlight are left out.
//!
//! Units are the movie's: pixels of the 1120 x 720 HUD stage, y down. Colour transforms are
//! Flash's: `c' = c * mult + add` per channel, colours 0..255, alpha 0..1 (see [`ColorTransform`]).

/// The HUD movie's stage (`UIScreen.INIT_STAGE_WIDTH` / `HEIGHT`).
pub const STAGE_SIZE: [f32; 2] = [1120.0, 720.0];
/// Where the HUD movie places the meter's origin on its stage (placement of `boostMeterView`).
pub const STAGE_POSITION: [f32; 2] = [1010.0, 610.0];
/// `BoostMeterView`: `this.rotationX = -2; this.rotationY = 15`.
pub const ROTATION_DEG: [f32; 2] = [-2.0, 15.0];
/// `fillProgressBar.z = fillProgressBarTinted.z = boostTextField.z = -20` (towards the viewer).
pub const FRONT_Z: f32 = -20.0;
/// `updatePerspectiveProjection`: `focalLength = 500` (set after `fieldOfView = 120`, so it wins).
pub const FOCAL_LENGTH: f32 = 500.0;
/// `projectionCenter = (x - 50, y - 100)`, relative to the meter's position.
pub const PROJECTION_CENTER_OFFSET: [f32; 2] = [-50.0, -100.0];
/// Frames of the two `FrameBasedProgressBar` fill timelines.
pub const FILL_FRAMES: u32 = 101;
/// `GLOW_NORMAL` / `GLOW_MAX`: the number's black `GlowFilter` blur (px, 2 passes, strength 1).
pub const TEXT_GLOW_NORMAL: f32 = 6.0;
pub const TEXT_GLOW_MAX: f32 = 8.0;
/// Text colours of the fields as authored (`textColor`), 0..255.
pub const BOOST_TEXT_COLOR: [f32; 3] = [255.0, 255.0, 255.0]; // set to 16777215 by the script
pub const BACKGROUND_TEXT_COLOR: [f32; 3] = [51.0, 51.0, 51.0];
pub const LABEL_TEXT_COLOR: [f32; 3] = [102.0, 102.0, 102.0];
/// The label's text: `NSLOCTEXT("Hud", "Boost", "Boost").toUpperCase()` (English).
pub const LABEL_TEXT: &str = "BOOST";

/// A Flash `ColorTransform`: `rgb' = rgb * mult + add` (0..255, clamped), `a' = a * mult + add`
/// with `add[3]` in 0..255 like Flash's `alphaOffset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorTransform {
    pub mult: [f32; 4],
    pub add: [f32; 4],
}

impl ColorTransform {
    /// Flash's `new ColorTransform(redMultiplier, ..., alphaOffset)`.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(r: f32, g: f32, b: f32, a: f32, ro: f32, go: f32, bo: f32, ao: f32) -> ColorTransform {
        ColorTransform { mult: [r, g, b, a], add: [ro, go, bo, ao] }
    }

    pub const IDENTITY: ColorTransform = ColorTransform::new(1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0);

    /// Applies to a colour (0..255) and alpha (0..1); returns (0..255, 0..1), clamped.
    pub fn apply(&self, rgb: [f32; 3], alpha: f32) -> ([f32; 3], f32) {
        let c = |i: usize| (rgb[i] * self.mult[i] + self.add[i]).clamp(0.0, 255.0);
        ([c(0), c(1), c(2)], (alpha * self.mult[3] + self.add[3] / 255.0).clamp(0.0, 1.0))
    }

    fn lerp(&self, to: &ColorTransform, t: f32) -> ColorTransform {
        let mut out = *self;
        for i in 0..4 {
            out.mult[i] += (to.mult[i] - self.mult[i]) * t;
            out.add[i] += (to.add[i] - self.add[i]) * t;
        }
        out
    }

    /// The same transform with Flash's `alpha` property set (the alpha multiplier).
    pub fn with_alpha(mut self, alpha: f32) -> ColorTransform {
        self.mult[3] = alpha;
        self
    }
}

// BoostMeterView's colour constants.
pub const COLOR_NORMAL: ColorTransform = ColorTransform::IDENTITY;
pub const COLOR_WHITE: ColorTransform = ColorTransform::new(1.0, 1.0, 1.0, 1.0, 255.0, 255.0, 255.0, 0.0);
pub const COLOR_BOOST_MAX: ColorTransform = ColorTransform::new(0.0, 0.6, 0.49, 1.0, 255.0, 64.0, 64.0, 0.0);
pub const COLOR_BOOST_NORMAL: ColorTransform = ColorTransform::new(1.0, 0.38, 0.51, 1.0, 255.0, 127.0, 0.0, 0.0);
pub const COLOR_GLOW_AMBER: ColorTransform = ColorTransform::new(0.0, 0.0, 0.0, 0.25, 255.0, 147.0, 67.0, 0.0);
pub const COLOR_GLOW_RED: ColorTransform = ColorTransform::new(0.0, 0.0, 0.0, 0.75, 255.0, 64.0, 64.0, 0.0);
pub const COLOR_BACKGROUND_AMBER: ColorTransform = ColorTransform::new(1.0, 0.43, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0);

fn quad_out(t: f32) -> f32 {
    t * (2.0 - t)
}

fn strong_out(t: f32) -> f32 {
    let u = 1.0 - t;
    1.0 - u * u * u * u * u
}

/// A running TweenMax tween of one value.
#[derive(Clone, Copy, Debug)]
struct Tween<T> {
    from: T,
    to: T,
    elapsed: f32,
    duration: f32,
    strong: bool,
}

impl<T> Tween<T> {
    fn new(from: T, to: T, duration: f32, strong: bool) -> Tween<T> {
        Tween { from, to, elapsed: 0.0, duration, strong }
    }

    fn ratio(&self) -> f32 {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        if self.strong { strong_out(t) } else { quad_out(t) }
    }

    fn done(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// One colour transform property, possibly being tweened.
#[derive(Clone, Copy, Debug)]
struct CtProp {
    value: ColorTransform,
    tween: Option<Tween<ColorTransform>>,
}

impl CtProp {
    fn new(value: ColorTransform) -> CtProp {
        CtProp { value, tween: None }
    }

    fn tween_to(&mut self, to: ColorTransform, duration: f32, strong: bool) {
        self.tween = Some(Tween::new(self.value, to, duration, strong));
    }

    fn advance(&mut self, dt: f32) {
        if let Some(t) = &mut self.tween {
            t.elapsed += dt;
            self.value = t.from.lerp(&t.to, t.ratio());
            if t.done() {
                self.tween = None;
            }
        }
    }
}

/// What to draw this frame. Colour transforms already include each object's `alpha`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoostMeterFrame {
    /// Show the meter at all (`BoostMeterView.visible`; hosts may hide it themselves too).
    pub visible: bool,
    /// Frame (1..=101) of both fill timelines; frame 1 is empty, 101 the whole bitmap.
    pub fill_frame: u32,
    /// The number as displayed, ASCII (`String(boost)`, plus a trailing space after a two-digit
    /// number ending in 1, as the script does), `text_len` bytes.
    pub text: [u8; 4],
    pub text_len: usize,
    pub background: ColorTransform,
    pub glow: ColorTransform,
    /// `glowMovieClip.scaleX/Y` (about the meter's origin).
    pub glow_scale: f32,
    pub fill: ColorTransform,
    pub fill_tinted: ColorTransform,
    /// `backgroundTextField` (text colour [`BACKGROUND_TEXT_COLOR`]).
    pub background_text: ColorTransform,
    /// `boostLabel.textField` (text colour [`LABEL_TEXT_COLOR`]), times the label's blinking alpha.
    pub label: ColorTransform,
    /// `boostTextField` (text colour [`BOOST_TEXT_COLOR`]); applies to its glow too, which is
    /// black before the transform.
    pub boost_text: ColorTransform,
    /// The number's glow blur (px): [`TEXT_GLOW_NORMAL`] or [`TEXT_GLOW_MAX`].
    pub text_glow_blur: f32,
}

impl BoostMeterFrame {
    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.text[..self.text_len]).unwrap_or("")
    }
}

/// The state of one boost meter. Feed it the car's boost every rendered frame with
/// [`BoostMeterView::update`].
#[derive(Clone, Debug)]
pub struct BoostMeterView {
    /// `m_displayBoost` (an `int` in AS3).
    display_boost: i32,
    display_tween: Option<Tween<f32>>,
    /// `m_lastBoostValue`.
    last_boost: i32,
    /// The value last reported (`LocalCar.BoostPercentageChanged` fires on changes only).
    reported: Option<i32>,
    max_boost: bool,
    progress: Option<f32>,
    fill_frame: u32,
    text: [u8; 4],
    text_len: usize,
    glow: CtProp,
    glow_scale: f32,
    glow_scale_tween: Option<Tween<f32>>,
    fill: CtProp,
    fill_tinted: CtProp,
    boost_text: ColorTransform,
    background_text: ColorTransform,
    label_ct: ColorTransform,
    label_alpha: f32,
    label_tween: Option<Tween<f32>>,
    /// The label's tween is the endless yoyo (true) or a one-off fade out.
    label_blinking: bool,
    text_glow_blur: f32,
}

impl Default for BoostMeterView {
    fn default() -> Self {
        Self::new()
    }
}

impl BoostMeterView {
    /// The meter as `handleAddedToStage` leaves it, before the first boost value: amber look
    /// (`onTeamColorChanged` without team colours), number 0.
    pub fn new() -> BoostMeterView {
        BoostMeterView {
            display_boost: 0,
            display_tween: None,
            last_boost: 100,
            reported: None,
            max_boost: false,
            progress: None,
            fill_frame: 1,
            text: *b"0   ",
            text_len: 1,
            glow: CtProp::new(COLOR_GLOW_AMBER),
            glow_scale: 1.0,
            glow_scale_tween: None,
            fill: CtProp::new(COLOR_NORMAL),
            fill_tinted: CtProp::new(COLOR_BOOST_NORMAL),
            boost_text: COLOR_BOOST_NORMAL,
            background_text: COLOR_BOOST_NORMAL.with_alpha(0.5),
            label_ct: COLOR_BOOST_NORMAL,
            label_alpha: 0.0,
            label_tween: None,
            label_blinking: false,
            text_glow_blur: TEXT_GLOW_NORMAL, // the filter placed on the text field
        }
    }

    /// Advances `dt` seconds with the car's boost (0..=100) and returns what to draw.
    pub fn update(&mut self, boost_amount: f32, dt: f32) -> BoostMeterFrame {
        let value = boost_amount.clamp(0.0, 100.0) as i32;
        if self.reported != Some(value) {
            self.reported = Some(value);
            self.on_boost_changed(value);
        }
        self.advance(dt.max(0.0));
        self.frame()
    }

    /// The current look without advancing time.
    pub fn frame(&self) -> BoostMeterFrame {
        BoostMeterFrame {
            visible: true,
            fill_frame: self.fill_frame,
            text: self.text,
            text_len: self.text_len,
            background: COLOR_BACKGROUND_AMBER,
            glow: self.glow.value,
            glow_scale: self.glow_scale,
            fill: self.fill.value,
            fill_tinted: self.fill_tinted.value,
            background_text: self.background_text,
            label: self.label_ct.with_alpha(self.label_ct.mult[3] * self.label_alpha),
            boost_text: self.boost_text,
            text_glow_blur: self.text_glow_blur,
        }
    }

    /// `onBoostChanged(value, true)`.
    fn on_boost_changed(&mut self, value: i32) {
        self.display_tween = Some(Tween::new(self.display_boost as f32, value as f32, 0.2, false));
        if value > self.last_boost {
            self.fill.value = COLOR_WHITE;
            self.fill_tinted.value = COLOR_WHITE;
            self.fill.tween_to(COLOR_NORMAL, 0.8, true);
            // Always the max tint here (without team colours), whatever the current look.
            self.fill_tinted.tween_to(COLOR_BOOST_MAX, 0.8, true);
            self.glow_scale = 2.0;
            self.glow_scale_tween = Some(Tween::new(2.0, 1.0, 0.8, true));
        }
        self.last_boost = value;
    }

    fn advance(&mut self, dt: f32) {
        // TweenMax renders its tweens in creation order; the ones here do not interact.
        if let Some(t) = &mut self.display_tween {
            t.elapsed += dt;
            // Assigning a Number to an `int` property truncates.
            let v = (t.from + (t.to - t.from) * t.ratio()) as i32;
            let done = t.done();
            if done {
                self.display_tween = None;
            }
            self.display_boost = v;
            self.update_boost();
        }
        self.fill.advance(dt);
        self.fill_tinted.advance(dt);
        self.glow.advance(dt);
        if let Some(t) = &mut self.glow_scale_tween {
            t.elapsed += dt;
            self.glow_scale = t.from + (t.to - t.from) * t.ratio();
            if t.done() {
                self.glow_scale_tween = None;
            }
        }
        if let Some(t) = &mut self.label_tween {
            t.elapsed += dt;
            if self.label_blinking {
                // yoyo, repeat -1: every other 0.2 s cycle plays backwards.
                let cycle = (t.elapsed / t.duration) as u32;
                let u = (t.elapsed - cycle as f32 * t.duration) / t.duration;
                let r = if cycle.is_multiple_of(2) { quad_out(u) } else { quad_out(1.0 - u) };
                self.label_alpha = t.from + (t.to - t.from) * r;
            } else {
                self.label_alpha = t.from + (t.to - t.from) * t.ratio();
                if t.done() {
                    self.label_tween = None;
                }
            }
        }
    }

    /// `updateBoost`.
    fn update_boost(&mut self) {
        let mut s = [b' '; 4];
        let digits = format_int(self.display_boost, &mut s);
        let mut len = digits;
        if digits == 2 && s[1] == b'1' {
            s[2] = b' ';
            len = 3;
        }
        self.text = s;
        self.text_len = len;
        self.set_progress(self.display_boost as f32 / 100.0);
        if self.display_boost > 80 {
            if !self.max_boost {
                self.text_glow_blur = TEXT_GLOW_MAX;
                self.glow.tween_to(COLOR_GLOW_RED, 0.3, false);
                self.boost_text = COLOR_BOOST_MAX;
                self.fill_tinted.value = COLOR_BOOST_MAX;
                self.label_ct = COLOR_BOOST_MAX;
                self.background_text = COLOR_BOOST_MAX.with_alpha(0.5);
                self.label_alpha = 0.0;
                self.label_tween = Some(Tween::new(0.0, 1.0, 0.2, false));
                self.label_blinking = true;
                self.max_boost = true;
            }
        } else if self.max_boost {
            self.text_glow_blur = TEXT_GLOW_NORMAL;
            self.glow.tween_to(COLOR_GLOW_AMBER, 0.3, false);
            self.boost_text = COLOR_BOOST_NORMAL;
            self.fill_tinted.value = COLOR_BOOST_NORMAL;
            self.label_ct = COLOR_BOOST_NORMAL;
            self.background_text = COLOR_BOOST_NORMAL.with_alpha(0.5);
            self.label_tween = Some(Tween::new(self.label_alpha, 0.0, 0.2, false));
            self.label_blinking = false;
            self.max_boost = false;
        }
    }

    /// `FrameBasedProgressBar.setProgress(p)`: `gotoAndStop(int(p * totalFrames))` (frame 0 is
    /// taken as frame 1).
    fn set_progress(&mut self, p: f32) {
        if self.progress == Some(p) {
            return;
        }
        self.progress = Some(p);
        self.fill_frame = ((p * FILL_FRAMES as f32) as i32).clamp(1, FILL_FRAMES as i32) as u32;
    }
}

fn format_int(v: i32, out: &mut [u8; 4]) -> usize {
    let v = v.clamp(0, 999) as u32;
    let s = [(v / 100) as u8, (v / 10 % 10) as u8, (v % 10) as u8];
    let start = if v >= 100 { 0 } else if v >= 10 { 1 } else { 2 };
    for (i, d) in s[start..].iter().enumerate() {
        out[i] = b'0' + d;
    }
    3 - start
}

/// Where the meter is on screen, after the HUD scales its stage to the window
/// (`UIScreen.handleStageResized`) and moves the meter with the bottom-right corner
/// (`HudScreen.handleStageResized`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoostMeterLayout {
    /// Screen pixels per movie pixel (`UIScreen.ContentScale`).
    pub scale: f32,
    /// The meter's origin in movie pixels of the scaled stage (y down, from the top left).
    pub position: [f32; 2],
}

impl BoostMeterLayout {
    /// For a `width` x `height` pixel screen. `ui_scale` is Rocket League's "HUD scale"
    /// (Interface settings, 1 by default; `UIScreen.setUIScale`).
    pub fn new(width: f32, height: f32, ui_scale: f32) -> BoostMeterLayout {
        let [sw, sh] = STAGE_SIZE;
        let fit = if width / height >= sw / sh { height / sh } else { width / sw };
        let scale = (fit * ui_scale).max(0.5); // MIN_CONTENT_SCALE
        let (cw, ch) = (width / scale, height / scale);
        BoostMeterLayout { scale, position: [STAGE_POSITION[0] + cw - sw, STAGE_POSITION[1] + ch - sh] }
    }

    /// Projects a point of the meter clip (movie px, y down, z towards the screen) to screen
    /// pixels. Returns `[x, y, w]`: `x, y` in pixels from the top left, and `w`, the perspective
    /// divisor (`(focal + z) / focal`), for perspective-correct texturing (clip space `xyz * w`).
    pub fn project(&self, p: [f32; 3]) -> [f32; 3] {
        let (sx, cx) = ROTATION_DEG[0].to_radians().sin_cos();
        let (sy, cy) = ROTATION_DEG[1].to_radians().sin_cos();
        // Flash appends rotationX, then rotationY, then the translation.
        let (x, y, z) = (p[0], p[1] * cx - p[2] * sx, p[1] * sx + p[2] * cx);
        let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
        let (px, py) = (self.position[0] + x, self.position[1] + y);
        let c = [self.position[0] + PROJECTION_CENTER_OFFSET[0], self.position[1] + PROJECTION_CENTER_OFFSET[1]];
        let w = (FOCAL_LENGTH + z) / FOCAL_LENGTH;
        [(c[0] + (px - c[0]) / w) * self.scale, (c[1] + (py - c[1]) / w) * self.scale, w]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(view: &mut BoostMeterView, boost: f32, seconds: f32) -> BoostMeterFrame {
        let mut f = view.frame();
        for _ in 0..(seconds * 120.0) as usize {
            f = view.update(boost, 1.0 / 120.0);
        }
        f
    }

    #[test]
    fn counts_up_and_switches_to_max() {
        let mut v = BoostMeterView::new();
        let f = run(&mut v, 33.4, 0.5);
        assert_eq!(f.text(), "33");
        assert_eq!(f.fill_frame, 33); // int(0.33 * 101)
        assert_eq!(f.boost_text, COLOR_BOOST_NORMAL);
        let f = run(&mut v, 100.0, 1.0);
        assert_eq!(f.text(), "100");
        assert_eq!(f.fill_frame, 101);
        assert_eq!(f.boost_text, COLOR_BOOST_MAX);
        assert_eq!(f.glow, COLOR_GLOW_RED);
        assert_eq!(f.text_glow_blur, TEXT_GLOW_MAX);
        assert_eq!(f.glow_scale, 1.0);
        assert_eq!(f.fill, COLOR_NORMAL);
    }

    #[test]
    fn pickup_flashes_and_label_blinks() {
        let mut v = BoostMeterView::new();
        run(&mut v, 50.0, 1.0);
        let f = v.update(90.0, 1.0 / 120.0);
        assert!(f.glow_scale > 1.9 && f.fill.add[1] > 230.0, "flash starts white and big");
        let alphas: Vec<f32> = (0..96).map(|_| v.update(90.0, 1.0 / 120.0).label.mult[3]).collect();
        assert!(alphas.iter().any(|&a| a > 0.95) && alphas[48..].iter().any(|&a| a < 0.05), "label blinks");
    }

    #[test]
    fn eleven_gets_a_trailing_space() {
        let mut v = BoostMeterView::new();
        assert_eq!(run(&mut v, 21.0, 0.5).text(), "21 ");
        assert_eq!(run(&mut v, 1.0, 0.5).text(), "1");
    }

    #[test]
    fn layout_follows_the_bottom_right_corner() {
        let l = BoostMeterLayout::new(1920.0, 1080.0, 1.0);
        assert_eq!(l.scale, 1.5);
        assert!((l.position[0] - 1170.0).abs() < 1e-3 && (l.position[1] - 610.0).abs() < 1e-3);
        // The origin projects to the meter's position (no z): unchanged by the projection.
        let p = l.project([0.0, 0.0, 0.0]);
        assert!((p[0] - 1755.0).abs() < 1e-3 && (p[1] - 915.0).abs() < 1e-3 && p[2] == 1.0);
        // Rotated by +15 degrees about Y, the right side comes towards the viewer.
        assert!(l.project([100.0, 0.0, 0.0])[2] < 1.0);
    }
}
