//! The home page and its parts: the frame (`home_page`) and what it draws
//! in order (`home_parts`), the titled sections it is made of
//! (`page_section`), the tab strip and the cards/list switch
//! (`filter_tabs`, `view_switch`, `home_view_mode`), the catalog's sections
//! shared with Explore (`example_groups`), the sign-in line
//! (`sign_in_prompt`), the boards half (`online_boards`, `offline_boards`,
//! `boards_section`, the one card mount point `board_card_slot`,
//! `board_row`), the "Unlocking your boards" fold (`keys_fold`), and the projects half (`other_projects`,
//! `projects_tab_section`, `your_patterns`, `project_items`, `project_row`,
//! the add row `project_add_row`, and the page-wide drop and paste
//! `library_drop`).

pub(crate) mod board_card_slot;
pub(crate) mod board_row;
pub(crate) mod boards_section;
pub(crate) mod example_groups;
pub(crate) mod filter_tabs;
pub(crate) mod home_page;
pub(crate) mod home_parts;
pub(crate) mod home_view_mode;
pub(crate) mod keys_fold;
pub(crate) mod library_drop;
pub(crate) mod offline_boards;
pub(crate) mod online_boards;
pub(crate) mod other_projects;
pub(crate) mod page_section;
pub(crate) mod project_add_row;
pub(crate) mod project_items;
pub(crate) mod project_row;
pub(crate) mod projects_tab_section;
pub(crate) mod sign_in_prompt;
pub(crate) mod view_switch;
pub(crate) mod your_patterns;

pub use home_page::HomePage;
