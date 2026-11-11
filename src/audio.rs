use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::db::VocabEntry;
use crate::RESOURCE_PREFIX;

/// Audio playback is disabled in this build.
///
/// There is no TTS pipeline or bundled audio in the repository. The playback
/// code is kept behind this flag so a future voice pipeline can drop clips
/// into `data/audio/characters/<codepoint>.ogg` and re-enable it.
pub const ENABLED: bool = false;

pub struct Audio {
    stream: RefCell<Option<gtk::MediaFile>>,
}

impl Audio {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            stream: RefCell::new(None),
        })
    }

    pub fn has_character_clip(entry: &VocabEntry) -> bool {
        ENABLED && Self::clip_path(entry).is_some()
    }

    pub fn play_entry(&self, entry: &VocabEntry) {
        if !ENABLED {
            return;
        }
        let Some(path) = Self::clip_path(entry) else {
            return;
        };

        let file = gtk::MediaFile::for_resource(&path);
        file.play();
        *self.stream.borrow_mut() = Some(file);
    }

    fn clip_path(entry: &VocabEntry) -> Option<String> {
        let mut characters = entry.simplified.chars();
        let character = characters.next()?;
        if characters.next().is_some() {
            return None;
        }

        let path = format!(
            "{RESOURCE_PREFIX}/audio/characters/{:x}.ogg",
            character as u32
        );
        gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE)
            .ok()
            .map(|_| path)
    }
}
