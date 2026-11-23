# Glossary

**RommSaveSyncer**
The save-sync app that runs alongside the user's existing launcher on each device. It never launches games on its own initiative or manages the ROM library.
_Avoid_: launcher, client, frontend

**Launcher**
The user's existing program for browsing and starting games (for example ES-DE, Steam, Argosy). Not part of RommSaveSyncer.
_Avoid_: frontend (when meaning RommSaveSyncer)

**Device**
One machine running RommSaveSyncer (for example the Windows PC, the Bazzite box, the AYN Thor), registered with RomM under a name the user chooses.

**Save**
The in-game save data a game writes itself (battery save, memory card file, GameCube GCI). The only thing RommSaveSyncer syncs.
_Avoid_: save file (when a Save spans several files), progress

**Save State**
An emulator snapshot of the whole machine at one instant. Not synced.
_Avoid_: save, quicksave

**Game Match**
The link between a local ROM on a device and its game in RomM. Found automatically or set by the user as a Manual Match.

**Manual Match**
A Game Match the user chose because no automatic match was found. Remembered once chosen.

**Play Session**
The period from an emulator starting a game until that emulator exits. Reported to RomM, which records it as playtime for that Device.

**Wrapped Launch**
A game start that the Launcher routes through RommSaveSyncer, which syncs first, starts the emulator, and waits for it to exit.
_Avoid_: proxy launch, hook

**Unwrapped Launch**
A game start RommSaveSyncer only notices after the emulator is already running. No Pre-launch Sync is possible; RommSaveSyncer warns if RomM holds a newer Save.

**Save Location**
Where an emulator keeps its Saves on a device. Detected by RommSaveSyncer, and always overridable by the user.

**Local Backup**
A copy of a local Save RommSaveSyncer keeps on the device before replacing it.

**Save Version**
One of the earlier copies of a Save that RomM keeps, which the user can restore.

**Background Pull**
Downloading a newer RomM Save while no emulator is running, done only when the local Save is unchanged since the last sync.

**Pre-launch Sync**
Bringing a newer RomM Save onto the device before a Play Session starts.

**Post-exit Sync**
Sending a changed local Save to RomM after a Play Session ends.

**Conflict**
Both the local Save and the RomM Save changed since they were last in sync. Always resolved by the user, never automatically.
_Avoid_: collision, mismatch
