//! Viewport → PTY resize policy.
//!
//! The remote terminal grid must follow genuine viewport changes
//! (rotation, split-view, window resize) but must **never** follow the
//! virtual keyboard: on iOS/Android the keyboard shrinks the viewport
//! height while the width stays identical, and resizing the remote PTY
//! around the keyboard destroys the scrollback layout the user is reading
//! (repo invariant: keyboard focus must not resize the remote grid).
//!
//! The platform shell (Xcode/Gradle wrapper, desktop window manager)
//! owns viewport events and calls into the portable core; this module is
//! the pure policy both shells share.

/// Decide whether a viewport change should resize the remote PTY.
///
/// `last` / `next` are `(cols, rows)` of the terminal grid fitted to the
/// viewport.
///
/// - identical size → no resize;
/// - degenerate (zero) size → no resize (never send a 0-column PTY);
/// - width unchanged → no resize: this is the virtual-keyboard signature
///   (height shrinks/grows, width identical), and may also be a
///   keyboard-avoidance inset on desktop — never a rotation;
/// - width changed → resize: rotation, split-view, or window resize.
pub fn should_resize_remote(last: (u16, u16), next: (u16, u16)) -> bool {
    if next == last {
        return false;
    }
    if next.0 == 0 || next.1 == 0 {
        return false;
    }
    next.0 != last.0
}

/// Fit a terminal grid into a viewport of `width_px` × `height_px` CSS
/// pixels given the cell size. Pure; the shell measures, this computes.
pub fn fit_grid(width_px: f64, height_px: f64, cell_w_px: f64, cell_h_px: f64) -> (u16, u16) {
    if cell_w_px <= 0.0 || cell_h_px <= 0.0 {
        return (0, 0);
    }
    let cols = (width_px / cell_w_px).floor().clamp(1.0, 1024.0) as u16;
    let rows = (height_px / cell_h_px).floor().clamp(1.0, 1024.0) as u16;
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_size_never_resizes() {
        assert!(!should_resize_remote((80, 24), (80, 24)));
    }

    #[test]
    fn keyboard_height_change_does_not_resize() {
        // Portrait iPhone: keyboard slides up, width identical.
        assert!(!should_resize_remote((80, 24), (80, 12)));
        // Keyboard dismisses again.
        assert!(!should_resize_remote((80, 12), (80, 24)));
    }

    #[test]
    fn rotation_resizes() {
        // Portrait → landscape: width changes.
        assert!(should_resize_remote((80, 24), (120, 18)));
        // Landscape → portrait.
        assert!(should_resize_remote((120, 18), (80, 24)));
    }

    #[test]
    fn degenerate_size_never_resizes() {
        assert!(!should_resize_remote((80, 24), (0, 0)));
        assert!(!should_resize_remote((80, 24), (80, 0)));
    }

    #[test]
    fn fit_grid_computes_columns_and_rows() {
        assert_eq!(fit_grid(800.0, 600.0, 10.0, 20.0), (80, 30));
    }

    #[test]
    fn fit_grid_clamps_and_rejects_bad_cells() {
        assert_eq!(fit_grid(800.0, 600.0, 0.0, 20.0), (0, 0));
        assert_eq!(fit_grid(100000.0, 100000.0, 1.0, 1.0), (1024, 1024));
    }
}
