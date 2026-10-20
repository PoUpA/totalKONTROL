# Profiles for DAWs and DJ software

totalKONTROL ships profiles for Ableton Live, Bitwig Studio, Cubase, Traktor Pro and Mixxx: in the **Profiles** tab, pick one in the preset list, click **Use**, then **Play this profile**. None of these programs knows the padKONTROL; each profile follows the program's conventions:

- **What works at once**: the drum pads (each program's drum instrument starts at C1 = note 36), and the transport through the program's own keyboard shortcuts (`key` actions: the program must be in front).
- **What you map once**: the knobs, the XY pad, the encoder and the mixer pads send plain CCs and notes, for the program's MIDI learn.

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

## Others worth adding

Profiles that could follow the same way: FL Studio, REAPER, Logic Pro, Serato DJ and rekordbox (DJ layout and MIDI learn), and, for keyboard shortcuts rather than MIDI, OBS (scenes, recording) or video editors.
