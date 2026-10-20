# Profiles for DAWs and DJ software

totalKONTROL ships profiles for Ableton Live, Bitwig Studio, Cubase, FL Studio, Logic Pro, REAPER, Traktor Pro, Mixxx, Serato DJ, rekordbox and OBS Studio: in the **Profiles** tab, pick one in the preset list, click **Use**, then **Play this profile**. None of these programs knows the padKONTROL; each profile follows the program's conventions:

- **What works at once**: the drum pads (each program's drum instrument starts at C1 = note 36), and the transport through the program's own keyboard shortcuts (`key` actions: the program must be in front).
- **What you map once**: the knobs, the XY pad, the encoder and the mixer pads send plain CCs and notes, for the program's MIDI learn.

Every control of these profiles has a **label** saying what it does: it shows on the editor's front panel, and under the pads of the live view while the profile plays. Change them in the editor's **Description** field when you map a control to something else.

Every profile sends to the DAW port picked in the connection card (**Virtual port**, or loopMIDI's `totalKONTROL` on Windows); enable that port as a MIDI input in the program. Shortcuts use `cmdorctrl`: Cmd on macOS, Ctrl elsewhere.

## Ableton Live

Bank **Drum Rack** (display `DRM`), and bank **Mixer** (`MIX`); **SETTING** switches between them.

| Control           | Sends                                   | In Live                                    |
| ----------------- | --------------------------------------- | ------------------------------------------ |
| Pads (Drum Rack)  | notes 36-51, channel 10, C1 bottom left | a Drum Rack plays them as is               |
| Pads 1-8 (Mixer)  | CC 102-109 toggles, channel 1           | map them (MIDI Map mode): mute, solo, arm… |
| Pads 9-16 (Mixer) | notes 36-43, channel 2                  | map them: clip slots, scenes…              |
| Knobs 1, 2        | CC 21, 22 (Mixer: 26, 27), pickup on    | map them: macros, device parameters…       |
| XY pad            | CC 23, 24 (Mixer: 28, 29)               | map them                                   |
| Encoder           | CC 25 (Mixer: 30), relative             | map it, mode _Relative (2's Comp.)_        |
| X                 | Space                                   | play / stop                                |
| Y                 | F9                                      | record                                     |
| SCENE             | Tab                                     | Session / Arrangement                      |
| MESSAGE           | Cmd/Ctrl+Z                              | undo                                       |
| PROG CHANGE       | Cmd/Ctrl+S                              | save                                       |
| ROLL, FLAM        | roll, flam                              |                                            |

To map: **Ctrl/Cmd+M** (MIDI Map mode), click a control in Live, touch the padKONTROL, **Ctrl/Cmd+M** again. For the Mixer pads' LEDs to follow Live, set the DAW feedback port in totalKONTROL, and turn **Remote** on for that port as a MIDI **output** in Live's preferences: Live then sends the mapped values back.

## Bitwig Studio

Same two banks as Live (**Drum Machine**, **Mixer**). Space plays and stops; **Y** sends CC 119, to learn as record; MESSAGE undoes, PROG CHANGE saves. Map the rest with a right-click on any control › **Learn Controller Assignment**, then touching the padKONTROL. Bitwig sends nothing back for learnt controls: the Mixer pads' LEDs keep their own state.

## Cubase

Banks **Groove Agent** (Groove Agent SE pads start at C1) and **Mixer**. The buttons use Cubase's default key commands:

| Button        | Key        | In Cubase          |
| ------------- | ---------- | ------------------ |
| X             | Space      | start / stop       |
| Y             | numpad *   | record             |
| SCENE         | numpad /   | cycle on / off     |
| MESSAGE       | numpad 1   | go to left locator |
| FIXED VEL     | numpad .   | return to zero     |
| KNOB 1 ASSIGN | Cmd/Ctrl+Z | undo               |
| PROG CHANGE   | Cmd/Ctrl+S | save               |

Map the CCs in **Studio › Studio Setup › Generic Remote** (or as Track Quick Controls). On a Mac keyboard without a numeric keypad, change the numpad keys in the profile.

## Traktor Pro

One bank, **Decks** (`TRK`), for decks A and B:

| Pads  | Notes (channel 1) | Map to                             |
| ----- | ----------------- | ---------------------------------- |
| Row 1 | 36-39             | deck A hot cues 1-4                |
| Row 2 | 40-43             | deck B hot cues 1-4                |
| Row 3 | 44, 45, 46, 47    | cue A, play A, cue B, play B       |
| Row 4 | 48, 49, 50, 51    | sync A, loop 4 A, sync B, loop 4 B |

X and Y (notes 52, 53): load the selected track on deck A, B. Knobs (CC 20, 21): filters A and B. XY pad: crossfader (CC 22) and CC 23. Encoder (CC 24, relative): browser list.

In **Preferences › Controller Manager**, **Add › Generic MIDI**, In-Port: the DAW port, Out-Port: the feedback port. Add each mapping with **Learn**; for play and sync, choose the **Toggle** interaction. The pads' LEDs are in `follow` mode: give the out mappings the same notes, and they light with Traktor's state.

## Mixxx

The **Mixxx** profile has the same layout as Traktor's, and Mixxx loads a mapping for it:

1. Copy [`mappings/mixxx/totalKONTROL.midi.xml`](../mappings/mixxx/totalKONTROL.midi.xml) to Mixxx's controllers folder: `%LOCALAPPDATA%\Mixxx\controllers` on Windows, `~/Library/Application Support/Mixxx/controllers` on macOS, `~/.mixxx/controllers` on Linux.
2. In **Preferences › Controllers**, select the DAW port, enable it and load **totalKONTROL (Korg padKONTROL)**.

The mapping does not send LED states back to the padKONTROL: play, sync and the loops flip their LED on each press, cues light while held. When they are changed from Mixxx's window, the LEDs no longer match until pressed again.

## FL Studio

Banks **FPC** and **Mixer**, like Live's. The buttons use FL Studio's default keys: X Space (play / stop), Y R (record), SCENE L (pattern / song mode), NOTE/CC# F5 (playlist), MIDI CH F6 (channel rack), SW TYPE F7 (piano roll), REL. VAL F9 (mixer), MESSAGE undo, PROG CHANGE save. In **Options › MIDI settings**, enable the DAW port as an input; map the CCs with a right-click on a control › **Link to controller**, then touching the padKONTROL. Set FPC's pads, or a channel's notes, to the profile's notes 36-51.

## REAPER

Banks **Drums** and **Mixer**. Buttons: X Space (play / stop), Y Ctrl/Cmd+R (record), SCENE R (repeat), NOTE/CC# Home (go to start), MIDI CH End (go to end), MESSAGE undo, FIXED VEL redo, PROG CHANGE save. Enable the DAW port in **Preferences › MIDI Devices** (input, and _Enable input for control messages_); map the CCs with a right-click on a control › **Learn**, or from the **Actions** list (_Add shortcut_, then touch the padKONTROL).

## Logic Pro

Banks **Drum Machine** (Drum Machine Designer starts at C1) and **Mixer**. Buttons: X Space (play / stop), Y R (record), SCENE C (cycle), NOTE/CC# Return (go to beginning), MIDI CH K (metronome), MESSAGE undo, FIXED VEL redo, PROG CHANGE save. Map the CCs with **Logic Pro › Control Surfaces › Learn Assignment** (Cmd+L): click a control in Logic, touch the padKONTROL.

## Serato DJ and rekordbox

The **Serato DJ** and **rekordbox** profiles have the DJ layout of Traktor's and Mixxx's (hot cues, cue, play, sync and loops for two decks, filters, crossfader, browse, load). Map them with the program's MIDI learn: in Serato DJ, the **MIDI** button of the setup screen, then click a control and touch the padKONTROL; in rekordbox, **MIDI** in the top bar, **LEARN**. Their LEDs keep their own state: play, sync and loops flip on each press, cues light while held.

## OBS Studio

OBS has no default hotkeys, so the **OBS Studio** profile types keys nothing else uses: pads 1-8 F13 to F20, pads 9-16 Shift+F13 to Shift+F20. In **Settings › Hotkeys**, click the field of an action and hit the pad:

| Pads | Keys      | Bind to (labels)            |
| ---- | --------- | --------------------------- |
| 1-8  | F13-F20   | scenes 1-8                  |
| 9    | Shift+F13 | start / stop recording      |
| 10   | Shift+F14 | start / stop streaming      |
| 11   | Shift+F15 | mute / unmute the mic       |
| 12   | Shift+F16 | mute / unmute desktop audio |
| 13   | Shift+F17 | save the replay buffer      |
| 14   | Shift+F18 | screenshot                  |
| 15   | Shift+F19 | studio mode                 |
| 16   | Shift+F20 | transition                  |

OBS must be in front for the keys to reach it, unless its hotkeys are set to work in the background.

## Others worth adding

The same way: Studio One, Pro Tools, Reason, Resolume and other VJ software (MIDI learn or OSC), Ableton Live clip launching with LED feedback through an OSC bridge such as AbletonOSC, and video editors (DaVinci Resolve, Premiere Pro) through their keyboard shortcuts.
