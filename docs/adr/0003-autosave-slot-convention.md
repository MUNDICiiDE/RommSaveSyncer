# Saves live in RomM's `autosave` slot

Each game's current Save is stored in RomM under `rom_id` plus the slot `autosave`, the same convention NeoStation and Lodor use. This lets the user switch between those apps, Argosy and RomM's web page without losing track of Saves. A private slot scheme was rejected because other clients would not see RommSaveSyncer's Saves.
