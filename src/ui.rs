use std::cmp::min;
use std::collections::HashMap;

use std::thread;
use std::sync::mpsc;
use std::time::Duration;

use pancurses::{Window, newwin, Input, Attribute};
use crate::config::Config;
use crate::keymap::{Keybindings, UserAction};
use crate::types::*;
use super::MainMessage;

/// Enum used for communicating back to the main controller after user
/// input has been captured by the UI. usize values always represent the
/// selected podcast, and (if applicable), the selected episode, in that
/// order.
#[derive(Debug)]
pub enum UiMsg {
    AddFeed(String),
    Play(usize, usize),
    MarkPlayed(usize, usize, bool),
    MarkAllPlayed(usize, bool),
    Sync(usize),
    SyncAll,
    Download(usize, usize),
    DownloadAll(usize),
    Quit,
    Noop,
}

/// Simple enum to identify which menu is currently active.
#[derive(Debug)]
enum ActiveMenu {
    PodcastMenu,
    EpisodeMenu,
}


/// Struct containing all interface elements of the TUI. Functionally, it
/// encapsulates the pancurses windows, and holds data about the size of
/// the screen.
#[derive(Debug)]
pub struct UI<'a> {
    stdscr: Window,
    n_row: i32,
    n_col: i32,
    keymap: &'a Keybindings,
    colors: Colors,
    podcast_menu: Menu<Podcast>,
    episode_menu: Menu<Episode>,
    active_menu: ActiveMenu,
    welcome_win: Option<Window>,
}

impl<'a> UI<'a> {
    /// Spawns a UI object in a new thread, with message channels to send
    /// and receive messages
    pub fn spawn(config: Config, items: LockVec<Podcast>, rx_from_main: mpsc::Receiver<MainMessage>, tx_to_main: mpsc::Sender<Message>) -> thread::JoinHandle<()> {
        return thread::spawn(move || {
            let mut ui = UI::new(&config, &items);
            let mut message_iter = rx_from_main.try_iter();
            // on each loop, we check for user input, then we process
            // any messages from the main thread
            loop {
                match ui.getch() {
                    UiMsg::Noop => (),
                    input => tx_to_main.send(Message::Ui(input)).unwrap(),
                }

                if let Some(message) = message_iter.next() {
                    match message {
                        MainMessage::UiUpdateMenus => ui.update_menus(),
                        MainMessage::UiSpawnMsgWin(msg, duration, error) => ui.spawn_msg_win(msg, duration, error),
                        MainMessage::UiTearDown => {
                            ui.tear_down();
                            break;
                        }
                    }
                }

                // slight delay to avoid excessive CPU usage
                thread::sleep(Duration::from_millis(10));
            }
        });
    }

    /// Initializes the UI with a list of podcasts and podcast episodes,
    /// creates the pancurses window and draws it to the screen, and
    /// returns a UI object for future manipulation.
    pub fn new(config: &'a Config, items: &LockVec<Podcast>) -> UI<'a> {
        let stdscr = pancurses::initscr();

        // set some options
        pancurses::cbreak();  // allows characters to be read one by one
        pancurses::noecho();  // turns off automatic echoing of characters
                              // to the screen as they are input
        pancurses::start_color();  // allows colours if available
        pancurses::curs_set(0);  // turn off cursor
        stdscr.keypad(true);  // returns special characters as single
                              // key codes
        stdscr.nodelay(true);  // getch() will not wait for user input

        // set colors
        let colors = set_colors();

        let (n_row, n_col) = stdscr.get_max_yx();

        let pod_col = n_col / 2;
        let ep_col = n_col - pod_col + 1;

        let podcast_menu_win = newwin(n_row - 1, pod_col, 0, 0);
        let mut podcast_menu = Menu {
            window: podcast_menu_win,
            screen_pos: 0,
            colors: colors.clone(),
            title: "Podcasts".to_string(),
            items: items.clone(),
            n_row: n_row - 3,  // 2 for border and 1 for messages at bottom
            n_col: pod_col - 5,  // 2 for border, 2 for margins
            top_row: 0,
            selected: 0,
        };

        stdscr.noutrefresh();
        podcast_menu.init();
        podcast_menu.activate();
        podcast_menu.window.noutrefresh();

        let episode_menu_win = newwin(n_row - 1, ep_col, 0, pod_col - 1);
        let first_pod: LockVec<Episode> = match items.borrow().get(0) {
            Some(pod) => pod.episodes.clone(),
            None => LockVec::new(Vec::new()),
        };
        let mut episode_menu = Menu {
            window: episode_menu_win,
            screen_pos: 1,
            colors: colors.clone(),
            title: "Episodes".to_string(),
            items: first_pod,
            n_row: n_row - 3,  // 2 for border and 1 for messages at bottom
            n_col: ep_col - 5,  // 2 for border, 2 for margins, and...
                                // 1 more for luck? I have no idea why
                                // this needs an extra 1, but it works
            top_row: 0,
            selected: 0,
        };
        episode_menu.init();
        episode_menu.window.noutrefresh();

        // welcome screen if user does not have any podcasts yet
        let welcome_win = if items.borrow().len() == 0 {
            Some(UI::make_welcome_win(&config.keybindings, n_row, n_col))
        } else {
            None
        };

        pancurses::doupdate();

        return UI {
            stdscr,
            n_row,
            n_col,
            keymap: &config.keybindings,
            colors: colors,
            podcast_menu: podcast_menu,
            episode_menu: episode_menu,
            active_menu: ActiveMenu::PodcastMenu,
            welcome_win: welcome_win,
        };
    }

    /// Waits for user input and, where necessary, provides UiMessages
    /// back to the main controller.
    /// 
    /// Anything UI-related (e.g., scrolling up and down menus) is handled
    /// internally, producing an empty UiMessage. This allows for some
    /// greater degree of abstraction; for example, input to add a new
    /// podcast feed spawns a UI window to capture the feed URL, and only
    /// then passes this data back to the main controller.
    pub fn getch(&mut self) -> UiMsg {
        match self.stdscr.getch() {
            Some(Input::KeyResize) => {
                pancurses::resize_term(0, 0);
                let (n_row, n_col) = self.stdscr.get_max_yx();
                self.n_row = n_row;
                self.n_col = n_col;

                let pod_col = n_col / 2;
                let ep_col = n_col - pod_col;
                self.podcast_menu.resize(n_row-3, pod_col-5);
                self.episode_menu.resize(n_row-3, ep_col-5);

                // apparently pancurses does not implement `wresize()`
                // from ncurses, so instead we create an entirely new
                // window every time the terminal is resized...not ideal,
                // but c'est la vie
                let pod_oldwin = std::mem::replace(
                    &mut self.podcast_menu.window,
                    newwin(n_row-1, pod_col, 0, 0));
                let ep_oldwin = std::mem::replace(
                    &mut self.episode_menu.window,
                    newwin(n_row-1, ep_col, 0, pod_col-1));
                pod_oldwin.delwin();
                ep_oldwin.delwin();
                self.stdscr.refresh();
                self.update_menus();
                
                match self.active_menu {
                    ActiveMenu::PodcastMenu => self.podcast_menu.activate(),
                    ActiveMenu::EpisodeMenu => {
                        self.podcast_menu.activate();
                        self.episode_menu.activate();
                    },
                }

                // resize welcome window, if it exists
                if self.welcome_win.is_some() {
                    let oldwwin = std::mem::replace(
                        &mut self.welcome_win,
                        Some(UI::make_welcome_win(&self.keymap, n_row, n_col)));
                    
                    oldwwin.unwrap().delwin();
                }
                self.stdscr.refresh();
            },

            Some(input) => {
                let pod_len = self.podcast_menu.items.borrow().len();
                let ep_len = self.episode_menu.items.borrow().len();
                let current_pod_index = (self.podcast_menu.selected +
                    self.podcast_menu.top_row) as usize;
                let current_ep_index = (self.episode_menu.selected +
                    self.episode_menu.top_row) as usize;

                // get rid of the "welcome" window once the podcast list
                // is no longer empty
                if self.welcome_win.is_some() && pod_len > 0 {
                    let ww = self.welcome_win.take().unwrap();
                    ww.delwin();
                }

                match self.keymap.get_from_input(input) {
                    Some(UserAction::Down) => {
                        match self.active_menu {
                            ActiveMenu::PodcastMenu => {
                                if pod_len > 0 {
                                    self.podcast_menu.scroll(1);

                                    self.episode_menu.top_row = 0;
                                    self.episode_menu.selected = 0;

                                    // update episodes menu with new list
                                    self.episode_menu.items = self.podcast_menu.get_episodes();
                                    self.episode_menu.update_items();
                                }
                            },
                            ActiveMenu::EpisodeMenu => {
                                if ep_len > 0 {
                                    self.episode_menu.scroll(1);
                                }
                            },
                        }
                    },

                    Some(UserAction::Up) => {
                        match self.active_menu {
                            ActiveMenu::PodcastMenu => {
                                if pod_len > 0 {
                                    self.podcast_menu.scroll(-1);

                                    self.episode_menu.top_row = 0;
                                    self.episode_menu.selected = 0;

                                    // update episodes menu with new list
                                    self.episode_menu.items = self.podcast_menu.get_episodes();
                                    self.episode_menu.update_items();
                                }
                            },
                            ActiveMenu::EpisodeMenu => {
                                if pod_len > 0 {
                                    self.episode_menu.scroll(-1);
                                }
                            },
                        }
                    },

                    Some(UserAction::Left) => {
                        if pod_len > 0 {
                            match self.active_menu {
                                ActiveMenu::PodcastMenu => (),
                                ActiveMenu::EpisodeMenu => {
                                    self.active_menu = ActiveMenu::PodcastMenu;
                                    self.podcast_menu.activate();
                                    self.episode_menu.deactivate();
                                },
                            }
                        }
                    },

                    Some(UserAction::Right) => {
                        if pod_len > 0 && ep_len > 0 {
                            match self.active_menu {
                                ActiveMenu::PodcastMenu => {
                                    self.active_menu = ActiveMenu::EpisodeMenu;
                                    self.podcast_menu.deactivate();
                                    self.episode_menu.activate();
                                },
                                ActiveMenu::EpisodeMenu => (),
                            }
                        }
                    },

                    Some(UserAction::AddFeed) => {
                        let url = &self.spawn_input_win("Feed URL: ");
                        if !url.is_empty() {
                            return UiMsg::AddFeed(url.to_string());
                        }
                    },

                    Some(UserAction::Sync) => {
                        if pod_len > 0 {
                            return UiMsg::Sync(current_pod_index);
                        }
                    },
                    Some(UserAction::SyncAll) => {
                        if pod_len > 0 {
                            return UiMsg::SyncAll;
                        }
                    },
                    Some(UserAction::Play) => {
                        if ep_len > 0 {
                            return UiMsg::Play(current_pod_index, current_ep_index);
                        }
                    },
                    Some(UserAction::MarkPlayed) => {
                        match self.active_menu {
                            ActiveMenu::PodcastMenu => (),
                            ActiveMenu::EpisodeMenu => {
                                if ep_len > 0 {
                                    let played = self.episode_menu.items
                                        .borrow()
                                        .get(current_ep_index).unwrap()
                                        .is_played();
                                    
                                    let attr = if played {
                                        pancurses::A_BOLD
                                    } else {
                                        pancurses::A_NORMAL
                                    };
                                    self.episode_menu.window.mvchgat(
                                        self.episode_menu.selected, 0, -1,
                                        attr, 2);
                                    self.episode_menu.window.refresh();
                                    return UiMsg::MarkPlayed(current_pod_index, current_ep_index, !played);
                                }
                            },
                        }
                    },
                    Some(UserAction::MarkAllPlayed) => {
                        // if there are any unplayed episodes, MarkAllPlayed
                        // will convert all to played; if all are played
                        // already, only then will it convert all to unplayed
                        if pod_len > 0 {
                            let played = self.podcast_menu.items
                                .borrow()
                                .get(current_pod_index).unwrap()
                                .is_played();
                            // let attr = if played {
                            //     pancurses::A_BOLD
                            // } else {
                            //     pancurses::A_NORMAL
                            // };

                            // // change attributes for selected podcast
                            // self.podcast_menu.window.mvchgat(
                            //     self.podcast_menu.selected, 0, -1,
                            //     attr, 2);

                            // // change attributes for all visible episodes
                            // let abs_bottom = min(self.episode_menu.n_row,
                            //     (self.episode_menu.items.lock().unwrap().len() - 1) as i32);
                            // for ep_i in 0..abs_bottom {
                            //     let color = if ep_i == self.episode_menu.selected {
                            //         2
                            //     } else {
                            //         1
                            //     };
                            //     self.episode_menu.window.mvchgat(
                            //         self.episode_menu.selected, 0, -1,
                            //         attr, color);
                            // }

                            // self.podcast_menu.window.refresh();
                            // self.episode_menu.window.refresh();
                            return UiMsg::MarkAllPlayed(current_pod_index, !played);
                        }
                    },

                    Some(UserAction::Download) => {
                        if ep_len > 0 {
                            return UiMsg::Download(current_pod_index, current_ep_index);
                        }
                    },

                    Some(UserAction::DownloadAll) => {
                        if pod_len > 0 {
                            return UiMsg::DownloadAll(current_pod_index);
                        }
                    },

                    Some(UserAction::Delete) => {},
                    Some(UserAction::DeleteAll) => {},
                    Some(UserAction::Remove) => {},
                    Some(UserAction::RemoveAll) => {},
                    Some(UserAction::Search) => {},

                    Some(UserAction::Quit) => {
                        return UiMsg::Quit;
                    },
                    None => (),
                }  // end of input match
            },
            None => (),
        };  // end of getch() match
        return UiMsg::Noop;
    }

    /// Adds a one-line pancurses window to the bottom of the screen to
    /// solicit user text input. A prefix can be specified as a prompt
    /// for the user at the beginning of the input line. This returns the
    /// user's input; if the user cancels their input, the String will be
    /// empty.
    pub fn spawn_input_win(&self, prefix: &str) -> String {
        let input_win = newwin(1, self.n_col, self.n_row-1, 0);
        // input_win.overlay(&self.podcast_menu.window);
        input_win.mv(self.n_row-1, 0);
        input_win.addstr(&prefix);
        input_win.keypad(true);
        input_win.refresh();
        pancurses::curs_set(2);
        
        let mut inputs = String::new();
        let mut cancelled = false;

        let min_x = prefix.len() as i32;
        let mut current_x = prefix.len() as i32;
        let mut cursor_x = prefix.len() as i32;
        loop {
            match input_win.getch() {
                // Cancel input
                Some(Input::KeyExit) |
                Some(Input::Character('\u{1b}')) => {
                    cancelled = true;
                    break;
                },
                // Complete input
                Some(Input::KeyEnter) |
                Some(Input::Character('\n')) => {
                    break;
                },
                Some(Input::KeyBackspace) |
                Some(Input::Character('\u{7f}')) => {
                    if current_x > min_x {
                        current_x -= 1;
                        cursor_x -= 1;
                        let _ = inputs.remove((cursor_x as usize) - prefix.len());
                        input_win.mv(0, cursor_x);
                        input_win.delch();
                    }
                },
                Some(Input::KeyDC) => {
                    if cursor_x < current_x {
                        let _ = inputs.remove((cursor_x as usize) - prefix.len());
                        input_win.delch();
                    }
                },
                Some(Input::KeyLeft) => {
                    if cursor_x > min_x {
                        cursor_x -= 1;
                        input_win.mv(0, cursor_x);
                    }
                },
                Some(Input::KeyRight) => {
                    if cursor_x < current_x {
                        cursor_x += 1;
                        input_win.mv(0, cursor_x);
                    }
                },
                Some(Input::Character(c)) => {
                    current_x += 1;
                    cursor_x += 1;
                    input_win.insch(c);
                    input_win.mv(0, cursor_x);
                    inputs.push(c);
                },
                Some(_) => (),
                None => (),
            }
            input_win.refresh();
        }

        pancurses::curs_set(0);
        input_win.deleteln();
        input_win.refresh();
        input_win.delwin();

        if cancelled {
            return String::from("");
        }
        return inputs;
    }

    /// Adds a one-line pancurses window to the bottom of the screen for
    /// displaying messages to the user. `duration` indicates how long
    /// (in milliseconds) this message will remain on screen. Useful for
    /// presenting error messages, among other things.
    pub fn spawn_msg_win(&self, message: String, duration: u64, error: bool) {
        let n_col = self.n_col;
        let begy = self.n_row - 1;
        let err_color = self.colors.get(ColorType::Error);
        thread::spawn(move || {
            let msg_win = newwin(1, n_col, begy, 0);
            msg_win.mv(begy, 0);
            msg_win.attrset(pancurses::A_NORMAL);
            msg_win.addstr(message);

            if error {
                msg_win.mvchgat(0, 0, -1, pancurses::A_BOLD,
                    err_color);
            }
            msg_win.refresh();

            // TODO: This probably should be some async function, but this
            // works for now
            // pancurses::napms(duration);
            thread::sleep(Duration::from_millis(duration));
            
            msg_win.erase();
            msg_win.refresh();
            msg_win.delwin();
        });
    }

    /// Forces the menus to check the list of podcasts/episodes again and
    /// update.
    pub fn update_menus(&mut self) {
        self.podcast_menu.update_items();
        self.episode_menu.update_items();
    }
    
    /// When the program is ending, this performs tear-down functions so
    /// that the terminal is properly restored to its prior settings.
    pub fn tear_down(&self) {
        pancurses::endwin();
    }

    /// Creates a pancurses window with a welcome message for when users
    /// start the program for the first time. Responsibility for managing
    /// the window is given back to the main UI object.
    pub fn make_welcome_win(keymap: &Keybindings,
        n_row: i32, n_col:i32) -> Window {

        let add_keys = keymap.keys_for_action(UserAction::AddFeed);
        let quit_keys = keymap.keys_for_action(UserAction::Quit);

        let add_str = match add_keys.len() {
            0 => "<missing>".to_string(),
            1 => format!("\"{}\"", &add_keys[0]),
            2 => format!("\"{}\" or \"{}\"", add_keys[0], add_keys[1]),
            _ => {
                let mut s = "".to_string();
                for i in 0..add_keys.len() {
                    if i == add_keys.len() - 1 {
                        s = format!("{}, \"{}\"", s, add_keys[i]);
                    } else {
                        s = format!("{}, or \"{}\"", s, add_keys[i]);
                    }
                }
                s
            }
        };

        let quit_str = match quit_keys.len() {
            0 => "<missing>".to_string(),
            1 => format!("\"{}\"", &quit_keys[0]),
            2 => format!("\"{}\" or \"{}\"", quit_keys[0], quit_keys[1]),
            _ => {
                let mut s = "".to_string();
                for i in 0..quit_keys.len() {
                    if i == quit_keys.len() - 1 {
                        s = format!("{}, \"{}\"", s, quit_keys[i]);
                    } else {
                        s = format!("{}, or \"{}\"", s, quit_keys[i]);
                    }
                }
                s
            }
        };

        let welcome_win = newwin(n_row-1, n_col, 0, 0);
        welcome_win.border(
            pancurses::ACS_VLINE(),
            pancurses::ACS_VLINE(),
            pancurses::ACS_HLINE(),
            pancurses::ACS_HLINE(),
            pancurses::ACS_ULCORNER(),
            pancurses::ACS_URCORNER(),
            pancurses::ACS_LLCORNER(),
            pancurses::ACS_LRCORNER());
        welcome_win.mvaddstr(0, 2, "Shellcaster");
        welcome_win.mvaddstr(2, 2, "Welcome to shellcaster!");
        welcome_win.mvaddstr(4, 2, format!("Your podcast list is currently empty. Press {} to add a new podcast feed, or {} to quit.", add_str, quit_str));
        welcome_win.mvaddstr(6, 2, "Other keybindings can be found on the Github repo readme:");
        welcome_win.mvaddstr(7, 2, "https://github.com/jeff-hughes/shellcaster");
        welcome_win.refresh();
        return welcome_win;
    }
}

/// Generic struct holding details about a list menu. These menus are
/// contained by the UI, and hold the list of podcasts or podcast
/// episodes. They also hold the pancurses window used to display the menu
/// to the user.
///
/// * `screen_pos` stores the position of the window on the screen, from
///   left to right 
/// * `n_row` and `n_col` store the size of the `window`
/// * `top_row` indicates the top line of text that is shown on screen
///   (since the list of items can be longer than the available size of
///   the screen). `top_row` is calculated relative to the `items` index,
///   i.e., it will be a value between 0 and items.len()
/// * `selected` indicates which item on screen is currently highlighted.
///   It is calculated relative to the screen itself, i.e., a value between
///   0 and (n_row - 1)
#[derive(Debug)]
pub struct Menu<T>
    where T: Clone + Menuable {
    window: Window,
    screen_pos: usize,
    colors: Colors,
    title: String,
    items: LockVec<T>,
    n_row: i32,
    n_col: i32,
    top_row: i32,  // top row of text shown in window
    selected: i32,  // which line of text is highlighted
}

impl<T: Clone + Menuable> Menu<T> {
    /// Prints the list of visible items to the pancurses window and
    /// refreshes it.
    pub fn init(&mut self) {
        self.draw_border();
        self.update_items();
    }

    /// Draws a border around the window.
    fn draw_border(&self) {
        let top_left;
        let bot_left;
        match self.screen_pos {
            0 => {
                top_left = pancurses::ACS_ULCORNER();
                bot_left = pancurses::ACS_LLCORNER();
            }
            _ => {
                top_left = pancurses::ACS_TTEE();
                bot_left = pancurses::ACS_BTEE();
            }
        }
        self.window.border(
            pancurses::ACS_VLINE(),
            pancurses::ACS_VLINE(),
            pancurses::ACS_HLINE(),
            pancurses::ACS_HLINE(),
            top_left,
            pancurses::ACS_URCORNER(),
            bot_left,
            pancurses::ACS_LRCORNER());

        self.window.mvaddstr(0, 2, self.title.clone());
    }

    /// Prints or reprints the list of visible items to the pancurses
    /// window and refreshes it.
    fn update_items(&mut self) {
        self.window.erase();
        self.draw_border();

        if self.items.borrow().is_empty() {
            self.selected = -1;
        } else {
            if self.selected == -1 {
                self.selected = 0;
            }

            // for visible rows, print strings from list
            for i in 0..self.n_row {
                let item_idx = (self.top_row + i) as usize;
                if let Some(elem) = self.items.borrow().get(item_idx) {
                    // look for any unplayed episodes
                    let unplayed = !elem.is_played();
                    self.window.mv(self.abs_y(i), self.abs_x(0));
                    if unplayed {
                        self.window.attron(Attribute::Bold);
                    }
                    self.window.addstr(elem.get_title(self.n_col as usize));
                    if unplayed {
                        self.window.attroff(Attribute::Bold);
                    }
                } else {
                    break;
                }
            }
        }
        self.window.refresh();
    }

    /// Scrolls the menu up or down by `lines` lines. Negative values of
    /// `lines` will scroll the menu up.
    /// 
    /// This function examines the new selected value, ensures it does
    /// not fall out of bounds, and then updates the pancurses window to
    /// represent the new visible list.
    fn scroll(&mut self, lines: i32) {
        // this happens when there are no items in the list yet
        if self.selected == -1 {
            return;
        }

        // TODO: currently only handles scroll value of 1; need to extend
        // to be able to scroll multiple lines at a time
        let mut old_selected = self.selected;
        self.selected += lines;

        // don't allow scrolling past last item in list (if shorter than
        // self.n_row)
        let abs_bottom = min(self.n_row,
            (self.items.borrow().len() - 1) as i32);
        if self.selected > abs_bottom {
            self.selected = abs_bottom;
        }

        // scroll list if necessary:
        // scroll down
        if self.selected > (self.n_row - 1) {
            self.selected = self.n_row - 1;
            if let Some(elem) = self.items.borrow().get((self.top_row + self.n_row) as usize) {
                self.top_row += 1;
                self.window.mv(self.abs_y(0), self.abs_x(0));
                self.window.deleteln();
                old_selected -= 1;

                self.window.mv(self.abs_y(self.n_row-1), self.abs_x(-1));
                self.window.clrtobot();
                self.window.mvaddstr(self.abs_y(self.n_row-1), self.abs_x(0), elem.get_title(self.n_col as usize));

                self.draw_border();
            }

        // scroll up
        } else if self.selected < 0 {
            self.selected = 0;
            if let Some(elem) = self.items.borrow().get((self.top_row - 1) as usize) {
                self.top_row -= 1;
                self.window.mv(self.abs_y(0), 0);
                self.window.insertln();
                old_selected += 1;

                self.window.mv(self.abs_y(0), self.abs_x(0));
                self.window.addstr(elem.get_title(self.n_col as usize));

                self.draw_border();
            }
        }

        let old_played = if self.items.borrow().get((self.top_row + old_selected) as usize).unwrap().is_played() {
            pancurses::A_NORMAL
        } else {
            pancurses::A_BOLD
        };
        let new_played = if self.items.borrow().get((self.top_row + self.selected) as usize).unwrap().is_played() {
            pancurses::A_NORMAL
        } else {
            pancurses::A_BOLD
        };

        self.window.mvchgat(self.abs_y(old_selected), self.abs_x(-1),
            self.n_col+3,
            old_played,
            self.colors.get(ColorType::Normal));
        self.window.mvchgat(self.abs_y(self.selected), self.abs_x(-1),
            self.n_col+3,
            new_played,
            self.colors.get(ColorType::HighlightedActive));
        self.window.refresh();
    }

    /// Controls how the window changes when it is active (i.e., available
    /// for user input to modify state).
    fn activate(&mut self) {
        if self.selected > -1 {
            let played = if self.items.borrow().get(self.selected as usize).unwrap().is_played() {
                pancurses::A_NORMAL
            } else {
                pancurses::A_BOLD
            };
            self.window.mvchgat(self.abs_y(self.selected), self.abs_x(-1),
                self.n_col + 3,
                played,
                self.colors.get(ColorType::HighlightedActive));
            self.window.refresh();
        }
    }

    /// Updates window size
    fn resize(&mut self, n_row: i32, n_col: i32) {
        self.n_row = n_row;
        self.n_col = n_col;

        // if resizing moves selected item off screen, scroll the list
        // upwards to keep same item selected
        if self.selected > (self.n_row - 1) {
            self.top_row = self.top_row + self.selected - (self.n_row - 1);
            self.selected = self.n_row - 1;
        }
    }

    /// Calculates the y-value relative to the window rather than to the
    /// menu (i.e., taking into account borders and margins).
    fn abs_y(&self, y: i32) -> i32 {
        return y + 1;
    }

    /// Calculates the x-value relative to the window rather than to the
    /// menu (i.e., taking into account borders and margins).
    fn abs_x(&self, x: i32) -> i32 {
        return x + 2;
    }
}


impl Menu<Podcast> {
    /// Returns a cloned reference to the list of episodes from the
    /// currently selected podcast.
    pub fn get_episodes(&self) -> LockVec<Episode> {
        let index = self.selected + self.top_row;
        return self.items.borrow()
            .get(index as usize).unwrap().episodes.clone();
    }

    /// Controls how the window changes when it is inactive (i.e., not
    /// available for user input to modify state).
    fn deactivate(&mut self) {
        if self.selected > -1 {
            let played = if self.items.borrow().get(self.selected as usize).unwrap().is_played() {
                pancurses::A_NORMAL
            } else {
                pancurses::A_BOLD
            };
            self.window.mvchgat(self.abs_y(self.selected), self.abs_x(-1),
                self.n_col + 3,
                played,
                self.colors.get(ColorType::Highlighted));
            self.window.refresh();
        }
    }
}

impl Menu<Episode> {
    /// Controls how the window changes when it is inactive (i.e., not
    /// available for user input to modify state).
    fn deactivate(&mut self) {
        if self.selected > -1 {
            let played = if self.items.borrow().get(self.selected as usize).unwrap().is_played() {
                pancurses::A_NORMAL
            } else {
                pancurses::A_BOLD
            };
            self.window.mvchgat(self.abs_y(self.selected), self.abs_x(-1),
                self.n_col + 3,
                played,
                self.colors.get(ColorType::Normal));
            self.window.refresh();
        }
    }
}

// Everything to do with colors ----------------------------------------

/// Enum identifying relevant text states that will be associated with
/// distinct colors.
#[derive(Eq, PartialEq, Hash, Copy, Clone, Debug)]
enum ColorType {
    Normal,
    Highlighted,
    HighlightedActive,
    Error,
}

/// Keeps a hashmap associating ColorTypes with ncurses color pairs.
#[derive(Debug, Clone)]
struct Colors {
    map: HashMap<ColorType, i16>,
}

impl Colors {
    fn new() -> Colors {
        return Colors {
            map: HashMap::new(),
        }
    }

    fn insert(&mut self, color: ColorType, num: i16) {
        self.map.insert(color, num);
    }

    fn get(&self, color: ColorType) -> i16 {
        return *self.map.get(&color).unwrap();
    }
}


/// Sets up hashmap for ColorTypes in app, initiates color palette, and
/// sets up ncurses color pairs.
fn set_colors() -> Colors {
    // set up a hashmap for easier reference
    let mut colors = Colors::new();
    colors.insert(ColorType::Normal, 0);
    colors.insert(ColorType::Highlighted, 1);
    colors.insert(ColorType::HighlightedActive, 2);
    colors.insert(ColorType::Error, 3);

    // specify some colors by RGB value
    pancurses::init_color(pancurses::COLOR_WHITE, 680, 680, 680);
    pancurses::init_color(pancurses::COLOR_YELLOW, 820, 643, 0);

    // instantiate curses color pairs
    pancurses::init_pair(colors.get(ColorType::Normal),
        pancurses::COLOR_WHITE,
        pancurses::COLOR_BLACK);
    pancurses::init_pair(colors.get(ColorType::Highlighted),
        pancurses::COLOR_BLACK,
        pancurses::COLOR_WHITE);
    pancurses::init_pair(colors.get(ColorType::HighlightedActive),
        pancurses::COLOR_BLACK,
        pancurses::COLOR_YELLOW);
    pancurses::init_pair(colors.get(ColorType::Error),
        pancurses::COLOR_RED,
        pancurses::COLOR_BLACK);

    return colors;
}