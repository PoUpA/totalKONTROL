#!/usr/bin/env node
// LED light show for totalKONTROL's OSC input, to try it out.
//
// Sends <prefix>/led/pad/N, <prefix>/led/button/NAME and <prefix>/display to
// the app, cycling through a few animations. When the app's OSC output is on,
// it also listens to the mirror: hitting a pad sends a ripple from that pad.
//
// In totalKONTROL: connect in native mode with a profile, turn on "OSC
// output" (host 127.0.0.1, port 9000) and "Listen for OSC on port" (9001).
// Then: node tools/osc-led-demo.mjs   (Ctrl+C to stop)
//
// Options: --host 127.0.0.1 --port 9001 --listen 9000 --prefix /totalkontrol
//          --fps 20 --seconds 6   (--listen 0 turns the ripples off)

import dgram from "node:dgram";

const options = parseArgs(process.argv.slice(2), {
  host: "127.0.0.1",
  port: 9001,
  listen: 9000,
  prefix: "/totalkontrol",
  fps: 20,
  seconds: 6,
});

// --- OSC encoding (int and string arguments are all we need) ---------------

function oscString(text) {
  const bytes = Buffer.from(text + "\0", "utf8");
  return Buffer.concat([bytes, Buffer.alloc((4 - (bytes.length % 4)) % 4)]);
}

function encode(address, ...args) {
  const tags = "," + args.map((a) => (typeof a === "string" ? "s" : "i")).join("");
  const data = args.map((a) => {
    if (typeof a === "string") return oscString(a);
    const int = Buffer.alloc(4);
    int.writeInt32BE(a);
    return int;
  });
  return Buffer.concat([oscString(address), oscString(tags), ...data]);
}

/** Address and int arguments of a received message (the mirror only sends ints). */
function decode(buffer) {
  const readString = (offset) => {
    const end = buffer.indexOf(0, offset);
    return [buffer.toString("utf8", offset, end), Math.ceil((end + 1) / 4) * 4];
  };
  const [address, afterAddress] = readString(0);
  const [tags, afterTags] = readString(afterAddress);
  const args = [];
  let offset = afterTags;
  for (const tag of tags.slice(1)) {
    if (tag !== "i" || offset + 4 > buffer.length) break;
    args.push(buffer.readInt32BE(offset));
    offset += 4;
  }
  return { address, args };
}

// --- padKONTROL layout --------------------------------------------------------

const PADS = Array.from({ length: 16 }, (_, i) => i + 1); // pad 1 top left, rows of 4
const row = (pad) => Math.floor((pad - 1) / 4);
const col = (pad) => (pad - 1) % 4;
const padAt = (r, c) => r * 4 + c + 1;
// Buttons in a rough left-to-right order, for the scanner.
const BUTTONS = [
  "setting", "x", "y", "pedal", "noteCc", "midiCh", "swType", "relVal", "velocity", "port",
  "hold", "flam", "roll", "scene", "message", "fixedVelocity", "progChange", "knob1Assign", "knob2Assign",
];
// Pads walked as a spiral from the top left corner inwards.
const SPIRAL = [1, 2, 3, 4, 8, 12, 16, 15, 14, 13, 9, 5, 6, 7, 11, 10];

// --- Animations: each returns the set of lit pads and buttons at time t ---------

const animations = [
  {
    name: "SNK",
    frame(t) {
      const head = Math.floor(t * 10) % SPIRAL.length;
      const pads = new Set();
      for (let i = 0; i < 5; i++) pads.add(SPIRAL[(head - i + SPIRAL.length) % SPIRAL.length]);
      return { pads, buttons: new Set() };
    },
  },
  {
    name: "RAN",
    drops: [],
    frame(t) {
      if (Math.random() < 0.25) this.drops.push({ col: Math.floor(Math.random() * 4), start: t });
      this.drops = this.drops.filter((d) => (t - d.start) * 8 < 5);
      const pads = new Set();
      for (const d of this.drops) {
        const r = Math.floor((t - d.start) * 8);
        if (r < 4) pads.add(padAt(r, d.col));
      }
      return { pads, buttons: new Set() };
    },
  },
  {
    name: "WAV",
    frame(t) {
      const front = (t * 6) % 10;
      const pads = new Set(PADS.filter((p) => Math.abs(row(p) + col(p) - front) < 1.2));
      return { pads, buttons: new Set() };
    },
  },
  {
    name: "SPK",
    frame() {
      const pads = new Set(PADS.filter(() => Math.random() < 0.2));
      const buttons = new Set(BUTTONS.filter(() => Math.random() < 0.1));
      return { pads, buttons };
    },
  },
  {
    name: "SCN",
    frame(t) {
      const position = Math.floor(t * 12) % (BUTTONS.length + 4);
      const buttons = new Set([BUTTONS[position], BUTTONS[position - 1]].filter(Boolean));
      const column = position - BUTTONS.length;
      const pads = new Set(column >= 0 ? PADS.filter((p) => col(p) === column) : []);
      return { pads, buttons };
    },
  },
];

// --- Ripples from pads hit on the padKONTROL (via the mirror) -------------------

const ripples = [];

function rippleFrame(t) {
  const lit = new Set();
  for (let i = ripples.length - 1; i >= 0; i--) {
    const { pad, start } = ripples[i];
    const radius = (t - start) * 10;
    if (radius > 7) {
      ripples.splice(i, 1);
      continue;
    }
    for (const p of PADS) {
      const distance = Math.abs(row(p) - row(pad)) + Math.abs(col(p) - col(pad));
      if (Math.abs(distance - radius) < 0.8) lit.add(p);
    }
  }
  return lit;
}

// --- Output -------------------------------------------------------------------

const socket = dgram.createSocket("udp4");
const send = (address, ...args) => socket.send(encode(address, ...args), options.port, options.host);
const shown = new Map(); // LED address → last state sent, to send changes only

function show(pads, buttons) {
  const wanted = new Map();
  for (const p of PADS) wanted.set(`${options.prefix}/led/pad/${p}`, pads.has(p) ? 1 : 0);
  for (const b of BUTTONS) wanted.set(`${options.prefix}/led/button/${b}`, buttons.has(b) ? 1 : 0);
  for (const [address, state] of wanted) {
    if (shown.get(address) !== state) {
      send(address, state);
      shown.set(address, state);
    }
  }
}

if (options.listen > 0) {
  const listener = dgram.createSocket("udp4");
  listener.on("message", (buffer) => {
    const { address, args } = decode(buffer);
    const match = address.match(new RegExp(`^${options.prefix}/pad/(\\d+)$`));
    if (match && args[0] > 0) ripples.push({ pad: Number(match[1]), start: clock() });
  });
  listener.on("error", (error) => {
    console.warn(`Ripples off: cannot listen on port ${options.listen} (${error.message}).`);
    listener.close();
  });
  listener.bind(options.listen);
}

const started = Date.now();
const clock = () => (Date.now() - started) / 1000;
let current = -1;

const timer = setInterval(() => {
  const t = clock();
  const index = Math.floor(t / options.seconds) % animations.length;
  if (index !== current) {
    current = index;
    send(`${options.prefix}/display`, animations[index].name);
    console.log(`${animations[index].name}`);
  }
  const { pads, buttons } = animations[index].frame(t);
  for (const p of rippleFrame(t)) pads.add(p);
  show(pads, buttons);
}, 1000 / options.fps);

console.log(
  `Sending to ${options.host}:${options.port} (${options.prefix}).` +
    (options.listen > 0 ? ` Hit a pad for a ripple (mirror on port ${options.listen}).` : "") +
    " Ctrl+C to stop.",
);

process.on("SIGINT", () => {
  clearInterval(timer);
  show(new Set(), new Set());
  send(`${options.prefix}/display`, "   ");
  // Let the last packets leave before quitting.
  setTimeout(() => process.exit(0), 200);
  console.log("\nLEDs off. Switch bank on the padKONTROL to hand them back to the profile.");
});

function parseArgs(argv, defaults) {
  const result = { ...defaults };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].replace(/^--/, "");
    if (!(key in defaults) || argv[i + 1] === undefined) {
      console.error(`Unknown option ${argv[i]}. Options: ${Object.keys(defaults).map((k) => "--" + k).join(" ")}`);
      process.exit(1);
    }
    result[key] = typeof defaults[key] === "number" ? Number(argv[i + 1]) : argv[i + 1];
  }
  return result;
}
