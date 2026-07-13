use ratatui::layout::{Margin, Rect};

#[derive(Debug, Clone, Copy)]
pub struct ScrollMetrics {
    pub content_length: usize,
    pub viewport_length: usize,
    pub position: usize,
}

impl ScrollMetrics {
    pub fn needs_scrollbar(self) -> bool {
        self.content_length > self.viewport_length && self.viewport_length > 0
    }

    pub fn max_position(self) -> usize {
        self.content_length.saturating_sub(self.viewport_length)
    }

    /// `ScrollbarState::content_length` so the thumb reaches the track end at
    /// [`Self::max_position`]. Ratatui treats `content_length - 1` as the last
    /// position, so we pass `max_position + 1`.
    pub fn scrollbar_state_content_length(self) -> usize {
        self.max_position().saturating_add(1)
    }
}

/// Round half away from zero the same way ratatui's scrollbar does.
const fn rounding_divide(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        return 0;
    }
    (numerator + denominator / 2) / denominator
}

pub fn vertical_scrollbar_track(area: Rect) -> Rect {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let col = inner.columns().last().unwrap_or(inner);
    Rect {
        x: col.x,
        y: col.y.saturating_add(1),
        width: 1,
        height: col.height.saturating_sub(2),
    }
}

pub fn horizontal_scrollbar_track(area: Rect) -> Rect {
    let inner = area.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    let row = inner.rows().last().unwrap_or(inner);
    Rect {
        x: row.x.saturating_add(1),
        y: row.y,
        width: row.width.saturating_sub(2),
        height: 1,
    }
}

/// Thumb start and length matching ratatui's `Scrollbar::part_lengths` when the
/// state uses [`ScrollMetrics::scrollbar_state_content_length`].
pub fn vertical_thumb_bounds(track_len: usize, metrics: ScrollMetrics) -> (usize, usize) {
    if track_len == 0 || !metrics.needs_scrollbar() {
        return (0, track_len.max(1));
    }
    let max_position = metrics.max_position();
    let start_position = metrics.position.min(max_position);
    let viewport_length = metrics.viewport_length;
    let max_viewport_position = max_position.saturating_add(viewport_length);
    if max_viewport_position == 0 {
        return (0, track_len);
    }
    let thumb_length = rounding_divide(
        viewport_length.saturating_mul(track_len),
        max_viewport_position,
    )
    .clamp(1, track_len);
    let thumb_start = rounding_divide(
        start_position.saturating_mul(track_len),
        max_viewport_position,
    )
    .min(track_len.saturating_sub(thumb_length));
    (thumb_start, thumb_length)
}

pub fn horizontal_thumb_bounds(track_len: usize, metrics: ScrollMetrics) -> (usize, usize) {
    vertical_thumb_bounds(track_len, metrics)
}

pub fn thumb_rect(track: Rect, thumb_start: usize, thumb_len: usize, vertical: bool) -> Rect {
    if vertical {
        Rect {
            x: track.x,
            y: track.y.saturating_add(thumb_start as u16),
            width: track.width,
            height: thumb_len as u16,
        }
    } else {
        Rect {
            x: track.x.saturating_add(thumb_start as u16),
            y: track.y,
            width: thumb_len as u16,
            height: track.height,
        }
    }
}

/// Map a thumb-top track coordinate to a scroll position.
///
/// Uses the thumb travel range (`track_len - thumb_len`) so dragging the thumb
/// to either end of the track reaches `0` and [`ScrollMetrics::max_position`].
pub fn position_from_vertical_track_y(rel_y: u16, track_len: u16, metrics: ScrollMetrics) -> usize {
    let max_pos = metrics.max_position();
    if max_pos == 0 || track_len == 0 {
        return 0;
    }
    let (_, thumb_len) = vertical_thumb_bounds(track_len as usize, metrics);
    let thumb_travel = (track_len as usize).saturating_sub(thumb_len);
    if thumb_travel == 0 {
        return 0;
    }
    let thumb_top = (rel_y as usize).min(thumb_travel);
    rounding_divide(thumb_top.saturating_mul(max_pos), thumb_travel).min(max_pos)
}

pub fn position_from_horizontal_track_x(rel_x: u16, track_len: u16, metrics: ScrollMetrics) -> usize {
    position_from_vertical_track_y(rel_x, track_len, metrics)
}

pub fn vertical_scrollbar_hit(
    area: Rect,
    pos: ratatui::layout::Position,
    metrics: ScrollMetrics,
) -> Option<VerticalScrollHit> {
    if !metrics.needs_scrollbar() {
        return None;
    }
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let col = inner.columns().last()?;
    if pos.x != col.x {
        return None;
    }
    if pos.y == col.y {
        return Some(VerticalScrollHit::PageUp);
    }
    if pos.y + 1 == col.y + col.height {
        return Some(VerticalScrollHit::PageDown);
    }
    let track = vertical_scrollbar_track(area);
    if !track.contains(pos) {
        return None;
    }
    let rel_y = pos.y.saturating_sub(track.y);
    let (thumb_start, thumb_len) = vertical_thumb_bounds(track.height as usize, metrics);
    let thumb = thumb_rect(track, thumb_start, thumb_len, true);
    if thumb.contains(pos) {
        Some(VerticalScrollHit::Thumb {
            grab_offset: rel_y.saturating_sub(thumb_start as u16),
        })
    } else {
        Some(VerticalScrollHit::Track { rel_y })
    }
}

pub fn horizontal_scrollbar_hit(
    area: Rect,
    pos: ratatui::layout::Position,
    metrics: ScrollMetrics,
) -> Option<HorizontalScrollHit> {
    if !metrics.needs_scrollbar() {
        return None;
    }
    let inner = area.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    let row = inner.rows().last()?;
    if pos.y != row.y {
        return None;
    }
    if pos.x == row.x {
        return Some(HorizontalScrollHit::PageLeft);
    }
    if pos.x + 1 == row.x + row.width {
        return Some(HorizontalScrollHit::PageRight);
    }
    let track = horizontal_scrollbar_track(area);
    if !track.contains(pos) {
        return None;
    }
    let rel_x = pos.x.saturating_sub(track.x);
    let (thumb_start, thumb_len) = horizontal_thumb_bounds(track.width as usize, metrics);
    let thumb = thumb_rect(track, thumb_start, thumb_len, false);
    if thumb.contains(pos) {
        Some(HorizontalScrollHit::Thumb {
            grab_offset: rel_x.saturating_sub(thumb_start as u16),
        })
    } else {
        Some(HorizontalScrollHit::Track { rel_x })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VerticalScrollHit {
    PageUp,
    PageDown,
    Thumb { grab_offset: u16 },
    Track { rel_y: u16 },
}

#[derive(Debug, Clone, Copy)]
pub enum HorizontalScrollHit {
    PageLeft,
    PageRight,
    Thumb { grab_offset: u16 },
    Track { rel_x: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(content: usize, viewport: usize, position: usize) -> ScrollMetrics {
        ScrollMetrics {
            content_length: content,
            viewport_length: viewport,
            position,
        }
    }

    #[test]
    fn thumb_reaches_track_end_at_max_position() {
        let m = metrics(100, 20, 80);
        assert_eq!(m.max_position(), 80);
        let track_len = 40;
        let (start, len) = vertical_thumb_bounds(track_len, m);
        assert_eq!(start + len, track_len, "thumb must sit flush with track end");
    }

    #[test]
    fn thumb_starts_at_track_start_at_zero() {
        let m = metrics(100, 20, 0);
        let (start, len) = vertical_thumb_bounds(40, m);
        assert_eq!(start, 0);
        assert!(len >= 1);
        assert!(len < 40);
    }

    #[test]
    fn drag_to_track_end_reaches_max_position() {
        let m = metrics(100, 20, 0);
        let track_len = 40u16;
        let (_, thumb_len) = vertical_thumb_bounds(track_len as usize, m);
        let thumb_travel = track_len as usize - thumb_len;
        let position =
            position_from_vertical_track_y(thumb_travel as u16, track_len, m);
        assert_eq!(position, m.max_position());
    }

    #[test]
    fn drag_to_track_start_reaches_zero() {
        let m = metrics(100, 20, 40);
        assert_eq!(position_from_vertical_track_y(0, 40, m), 0);
    }

    #[test]
    fn thumb_position_round_trips_near_ends() {
        let m0 = metrics(200, 25, 0);
        let track = 50usize;
        let (start0, len0) = vertical_thumb_bounds(track, m0);
        assert_eq!(start0, 0);

        let max = m0.max_position();
        let m_end = metrics(200, 25, max);
        let (start_end, len_end) = vertical_thumb_bounds(track, m_end);
        assert_eq!(start_end + len_end, track);
        assert_eq!(len0, len_end, "thumb size must be stable across positions");

        let recovered = position_from_vertical_track_y(start_end as u16, track as u16, m_end);
        assert_eq!(recovered, max);
    }

    #[test]
    fn scrollbar_state_content_length_lets_thumb_reach_end() {
        let m = metrics(100, 20, 80);
        // Ratatui clamps position to content_length - 1.
        assert_eq!(m.scrollbar_state_content_length(), 81);
        assert_eq!(m.position.min(m.scrollbar_state_content_length() - 1), 80);
    }
}
