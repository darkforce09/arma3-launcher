//! Time-based tweens with WPF's `KeySpline` easing.

use eframe::egui::Context;

/// A cubic Bézier easing curve through (0,0), (x1,y1), (x2,y2), (1,1), like WPF's `KeySpline`.
#[derive(Clone, Copy)]
pub struct Spline(pub f32, pub f32, pub f32, pub f32);

/// `KeySpline="0,0.7,0.7,1"`: the navigation sliders.
pub const NAV_SPLINE: Spline = Spline(0.0, 0.7, 0.7, 1.0);

impl Spline {
    /// The eased progress for linear progress `t` in 0..=1.
    pub fn ease(self, t: f32) -> f32 {
        let Spline(x1, y1, x2, y2) = self;
        let bezier = |a: f32, b: f32, s: f32| {
            3.0 * a * s * (1.0 - s).powi(2) + 3.0 * b * s * s * (1.0 - s) + s.powi(3)
        };
        // Solve x(s) = t by bisection; x is monotonic for control points inside the unit square.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if bezier(x1, x2, mid) < t {
                lo = mid
            } else {
                hi = mid
            }
        }
        bezier(y1, y2, (lo + hi) / 2.0)
    }
}

/// A value that moves to new targets over `duration` seconds.
pub struct Tween {
    from: f32,
    to: f32,
    start: f64,
    duration: f64,
    spline: Spline,
}

impl Tween {
    pub fn new(value: f32, duration: f64, spline: Spline) -> Self {
        Self {
            from: value,
            to: value,
            start: 0.0,
            duration,
            spline,
        }
    }

    /// Starts moving to `target` (no-op when already heading there).
    pub fn set(&mut self, ctx: &Context, target: f32) {
        if target != self.to {
            self.from = self.value(ctx);
            self.to = target;
            self.start = ctx.input(|i| i.time);
        }
    }

    /// Jumps to `target` without animating.
    pub fn jump(&mut self, target: f32) {
        self.from = target;
        self.to = target;
    }

    /// The current value; schedules repaints while moving.
    pub fn value(&self, ctx: &Context) -> f32 {
        let elapsed = ctx.input(|i| i.time) - self.start;
        if elapsed >= self.duration || self.from == self.to {
            return self.to;
        }
        ctx.request_repaint();
        let t = (elapsed / self.duration) as f32;
        self.from + (self.to - self.from) * self.spline.ease(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_ends_are_fixed_and_nav_curve_leads() {
        let linear = Spline(0.0, 0.0, 1.0, 1.0);
        for spline in [NAV_SPLINE, linear] {
            assert!(spline.ease(0.0).abs() < 1e-3);
            assert!((spline.ease(1.0) - 1.0).abs() < 1e-3);
        }
        assert!((linear.ease(0.3) - 0.3).abs() < 1e-3);
        // An ease-out curve is ahead of linear in the first half.
        assert!(NAV_SPLINE.ease(0.3) > 0.5);
    }
}
