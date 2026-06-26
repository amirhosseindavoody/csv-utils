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

pub fn vertical_thumb_bounds(track_len: usize, metrics: ScrollMetrics) -> (usize, usize) {
    if track_len == 0 || !metrics.needs_scrollbar() {
        return (0, track_len.max(1));
    }
    let track_length = track_len as f64;
    let viewport_length = metrics.viewport_length as f64;
    let max_position = metrics.content_length.saturating_sub(1) as f64;
    let start_position = (metrics.position as f64).clamp(0.0, max_position);
    let max_viewport_position = max_position + viewport_length;
    let end_position = start_position + viewport_length;
    let thumb_start = start_position * track_length / max_viewport_position;
    let thumb_end = end_position * track_length / max_viewport_position;
    let thumb_start = thumb_start.round().clamp(0.0, track_length - 1.0) as usize;
    let thumb_end = thumb_end.round().clamp(0.0, track_length) as usize;
    let thumb_length = thumb_end.saturating_sub(thumb_start).max(1);
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

pub fn position_from_vertical_track_y(rel_y: u16, track_len: u16, metrics: ScrollMetrics) -> usize {
    let max_pos = metrics.max_position();
    if track_len <= 1 {
        return rel_y.min(max_pos as u16) as usize;
    }
    let ratio = (rel_y as f64 / f64::from(track_len.saturating_sub(1))).clamp(0.0, 1.0);
    (ratio * max_pos as f64).round() as usize
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
