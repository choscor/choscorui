//! Navigation controls that mount through the control host: breadcrumbs,
//! pagination, the navigation menu, and the sidebar.

mod breadcrumb;
mod navigation_menu;
mod pagination;

pub use breadcrumb::Breadcrumb;
pub use navigation_menu::{
    NAVIGATION_CLOSE_DELAY, NAVIGATION_OPEN_DELAY, NavigationLink, NavigationMenu,
};
pub use pagination::Pagination;
