pub mod render;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Now,
}

#[derive(Clone, Copy)]
pub struct UiState {
    page: Page,
}

impl UiState {
    pub fn new() -> Self {
        Self { page: Page::Now }
    }

    pub fn page(&self) -> Page {
        self.page
    }
}
