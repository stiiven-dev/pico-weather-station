use station_core::page_index_from_percent;

pub mod render;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Now,
    MinMax,
    Trend,
    About,
}

#[derive(Clone, Copy)]
pub struct UiState {
    pub page: Page,
    pub frozen: bool,
}

impl Page {
    pub const COUNT: usize = 4;

    fn from_index(idx: usize) -> Self {
        match idx {
            0 => Page::Now,
            1 => Page::MinMax,
            2 => Page::Trend,
            _ => Page::About,
        }
    }
}
impl UiState {
    pub const fn new() -> Self {
        Self {
            page: Page::Now,
            frozen: false,
        }
    }

    /// Call every tick with pot #1's current percentage. Updates the
    /// active page unless frozen — freezing locks the *page* too, not
    /// just the values on it, so bumping the pot while reading a page
    /// doesn't accidentally flip you away from it.
    pub fn update_page(&mut self, pot1_pct: u8) {
        if self.frozen {
            return;
        }
        self.page = Page::from_index(page_index_from_percent(pot1_pct, Page::COUNT));
    }

    pub fn toggle_freeze(&mut self) {
        self.frozen = !self.frozen;
    }

    pub fn page(&self) -> Page {
        self.page
    }
}
