/* Pelican: the window.
 *
 * Plain JS, no build. Talks to the Tauri shell over IPC (status, places,
 * library_list, preview, push, stop, watch_list, ledger, udev_rule_status,
 * install_udev_rule, default_backup_dir, backup_watch, reset_check,
 * reset_ledger, and the "pelican://progress" and "pelican://backup"
 * events). Opened in a plain browser it loads demo.js, which serves the
 * same contract from fixtures and marks the window "Demo data".
 */
(() => {
  "use strict";

  // ------------------------------------------------------------ helpers

  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

  const ESC = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ESC[c]);

  const mark = (kind) =>
    `<svg class="mk" aria-hidden="true" focusable="false"><use href="#m-${kind}"/></svg>`;

  // `?still` settles every entrance and bloom, as reduced motion does, so a
  // headless capture shows each state at rest.
  const STILL = new URLSearchParams(location.search).has("still");
  if (STILL) document.documentElement.classList.add("still");
  const reduceMotion = STILL ? { matches: true } : matchMedia("(prefers-reduced-motion: reduce)");

  // SI units, as GNOME shows them. One decimal below 100 of a unit.
  function bytes(n) {
    if (n == null) return "-";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let v = n;
    let u = 0;
    while (v >= 1000 && u < units.length - 1) {
      v /= 1000;
      u += 1;
    }
    const s = u === 0 || v >= 100 ? Math.round(v).toString() : v.toFixed(1);
    return `${s} ${units[u]}`;
  }

  const plural = (n, one, many = `${one}s`) => `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
  const basename = (p) => String(p).replace(/\/+$/, "").split("/").pop() || p;
  const dirname = (p) => String(p).replace(/\/+$/, "").split("/").slice(0, -1).join("/") || "/";

  // Wrap the fixes the shell names (the udev rule, the gvfs unmount) as code.
  const withCode = (text) =>
    esc(text).replace(/(udev\/70-garmin-mtp\.rules|gio mount -u\s+\S+)/g, "<code>$1</code>");

  function announce(msg) {
    const el = $("#announce");
    el.textContent = "";
    requestAnimationFrame(() => {
      el.textContent = msg;
    });
  }

  const MAX_OBJECTS = 500; // core watch::MAX_OBJECTS
  const RESERVE_BYTES = 2 * 1024 * 1024; // core transfer::FREE_MARGIN
  const PROFILE_BPS = 192000; // core preview::PROFILE_BPS

  // Roughly how many more tracks fit: free space at the one profile for a
  // 3.5-minute track, capped by the object limit. Review shows the real
  // estimate for what was chosen.
  const AVG_TRACK_BYTES = (210 * PROFILE_BPS) / 8;
  function roomFor(st) {
    const bySpace = Math.floor(Math.max(0, st.free_bytes - RESERVE_BYTES) / AVG_TRACK_BYTES);
    const bySlots = Math.max(0, (st.max_objects ?? MAX_OBJECTS) - (st.music_objects ?? 0));
    const n = Math.min(bySpace, bySlots);
    return n >= 50 ? Math.floor(n / 10) * 10 : n;
  }

  // ------------------------------------------------------------ the ink

  // A bounded canvas behind the watch. The field is rendered once from
  // domain-warped noise; the canvas animates only during a bloom (one per
  // verified track, a red thread per failure), which then settles into a
  // faint residue. The CSS breathe loop is separate. Reduced motion bakes
  // the residue without animating; a hidden window pauses a bloom.
  const Ink = (() => {
    const cv = $("#ink");
    const ctx = cv.getContext("2d");
    const base = document.createElement("canvas");
    const bctx = base.getContext("2d");
    const resid = document.createElement("canvas");
    const rctx = resid.getContext("2d");
    const BLOOM_MS = 2800;
    // Noise is computed on a coarse grid and scaled up smooth: ink has no
    // edges finer than this, and the field renders in tens of milliseconds.
    const CELL = 3;
    let W = 0;
    let H = 0;
    let dpr = 1;
    let raf = 0;
    let pausedAt = 0;
    let blooms = [];
    let bloomSeed = 11;

    const rng = (s) => () => {
      s = (s * 16807) % 2147483647;
      return (s - 1) / 2147483646;
    };

    // Value noise on a hashed lattice, five octaves, and the warp that turns
    // it into billowing ink (fbm fed through fbm).
    const hash = (x, y, k) => {
      let h = (x * 374761393 + y * 668265263 + k * 1442695041) | 0;
      h = Math.imul(h ^ (h >>> 13), 1274126177);
      return ((h ^ (h >>> 16)) >>> 0) / 4294967295;
    };
    function noise(x, y, k) {
      const xi = Math.floor(x);
      const yi = Math.floor(y);
      const xf = x - xi;
      const yf = y - yi;
      const u = xf * xf * (3 - 2 * xf);
      const v = yf * yf * (3 - 2 * yf);
      const a = hash(xi, yi, k);
      const b = hash(xi + 1, yi, k);
      const c = hash(xi, yi + 1, k);
      const d = hash(xi + 1, yi + 1, k);
      return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
    }
    function fbm(x, y, k) {
      let s = 0;
      let amp = 0.5;
      for (let o = 0; o < 5; o += 1) {
        s += amp * noise(x, y, k + o);
        x = x * 2.03 + 17.1;
        y = y * 2.03 + 3.7;
        amp *= 0.5;
      }
      return s / 0.97;
    }
    // Returns the warped density and the second warp's offset, which bends
    // the plume's envelope so its edge billows instead of ending in a curve.
    function warped(x, y, k) {
      const qx = fbm(x, y, k);
      const qy = fbm(x + 5.2, y + 1.3, k);
      const rx = fbm(x + 3.2 * qx + 1.7, y + 3.2 * qy + 9.2, k + 7);
      const ry = fbm(x + 3.2 * qx + 8.3, y + 3.2 * qy + 2.8, k + 7);
      return [fbm(x + 3 * rx, y + 3 * ry, k + 13), rx - 0.5, ry - 0.5];
    }

    const smooth = (a, b, x) => {
      const t = Math.max(0, Math.min(1, (x - a) / (b - a)));
      return t * t * (3 - 2 * t);
    };

    // The case, in canvas pixels.
    function geometry() {
      const f = cv.getBoundingClientRect();
      const c = $("#watch .case-ref").getBoundingClientRect();
      return {
        x: (c.left - f.left + c.width / 2) * dpr,
        y: (c.top - f.top + c.height / 2) * dpr,
        r: (c.width / 2) * dpr,
      };
    }

    function drawBase() {
      bctx.fillStyle = "#050506";
      bctx.fillRect(0, 0, W, H);
      const g = geometry();
      const step = CELL * dpr;
      const gw = Math.ceil(W / step) + 1;
      const gh = Math.ceil(H / step) + 1;
      const low = document.createElement("canvas");
      low.width = gw;
      low.height = gh;
      const lctx = low.getContext("2d");
      const img = lctx.createImageData(gw, gh);
      const px = img.data;
      for (let j = 0; j < gh; j += 1) {
        for (let i = 0; i < gw; i += 1) {
          // Watch-relative units: the plume keeps its shape at every size.
          const dx = (i * step - g.x) / g.r;
          const dy = (j * step - g.y) / g.r;
          const [f, wx, wy] = warped(dx * 0.46 + 40, dy * 0.46 + 20, 3);
          // The body: taller than wide, drifting up, its edge pushed about
          // by the warp so it feathers into clear black water.
          const ex = (dx + wx * 2.4) / 1.9;
          const ey = (dy + 0.3 + wy * 2.4) / 2.4;
          const env = smooth(1.05, 0.2, Math.sqrt(ex * ex + ey * ey));
          const dens = smooth(0.34, 0.78, f) * env;
          // The light inside the ink sits behind the case.
          const rr = dx * dx + dy * dy;
          const light = 0.12 + 0.88 * Math.exp(-rr / 1.6);
          // Denser folds catch more of it: backlit smoke, brighter at the
          // billow than in the thin water between.
          const lum = Math.min(1, dens * light * (0.55 + 0.9 * smooth(0.5, 0.88, f)) * 1.3);
          const o = (j * gw + i) * 4;
          px[o] = 5 + lum * 150;
          px[o + 1] = 5 + lum * 143;
          px[o + 2] = 6 + lum * 132;
          px[o + 3] = 255;
        }
      }
      lctx.putImageData(img, 0, 0);
      bctx.imageSmoothingEnabled = true;
      bctx.imageSmoothingQuality = "high";
      bctx.drawImage(low, 0, 0, gw * step, gh * step);
      // Fine grain so the ink is a material, not a smooth render.
      const rnd = rng(7);
      const tile = document.createElement("canvas");
      tile.width = 96;
      tile.height = 96;
      const tctx = tile.getContext("2d");
      const grain = tctx.createImageData(96, 96);
      for (let i = 0; i < grain.data.length; i += 4) {
        const v = rnd() > 0.5 ? 237 : 0;
        grain.data[i] = v;
        grain.data[i + 1] = v;
        grain.data[i + 2] = v;
        grain.data[i + 3] = rnd() * 9;
      }
      tctx.putImageData(grain, 0, 0);
      bctx.fillStyle = bctx.createPattern(tile, "repeat");
      bctx.fillRect(0, 0, W, H);
    }

    function compose() {
      ctx.clearRect(0, 0, W, H);
      ctx.drawImage(base, 0, 0);
      ctx.drawImage(resid, 0, 0);
    }

    function resize() {
      const r = cv.getBoundingClientRect();
      dpr = Math.min(2, window.devicePixelRatio || 1);
      const w = Math.max(1, Math.round(r.width * dpr));
      const h = Math.max(1, Math.round(r.height * dpr));
      if (w === W && h === H) return;
      const keep = document.createElement("canvas");
      keep.width = W || 1;
      keep.height = H || 1;
      if (W) keep.getContext("2d").drawImage(resid, 0, 0);
      W = w;
      H = h;
      for (const c of [cv, base, resid]) {
        c.width = W;
        c.height = H;
      }
      drawBase();
      if (keep.width > 1) rctx.drawImage(keep, 0, 0, W, H);
      compose();
    }

    // One bloom: a plume leaving the case at a heading, its density and the
    // moment each cell of it arrives, both fixed up front. Drawing a frame
    // is then a pass over two arrays. A failure is a thread, not a plume.
    function newBloom(fail) {
      const g = geometry();
      const rnd = rng((bloomSeed = (bloomSeed * 48271) % 2147483647));
      const heading = rnd() * Math.PI * 2;
      const k = 50 + Math.floor(rnd() * 900);
      const span = g.r * 6;
      const step = 4 * dpr;
      const n = Math.ceil(span / step);
      const dens = new Float32Array(n * n);
      const at = new Float32Array(n * n);
      const hx = Math.cos(heading);
      const hy = Math.sin(heading);
      const width = fail ? 0.05 : 0.34;
      for (let j = 0; j < n; j += 1) {
        for (let i = 0; i < n; i += 1) {
          const dx = (i * step - span / 2) / g.r;
          const dy = (j * step - span / 2) / g.r;
          const [f, wx, wy] = warped(dx * (fail ? 1.1 : 0.8) + k, dy * (fail ? 1.1 : 0.8), k);
          // Along and across the heading, from the rim outward, bent by the warp.
          const ax = dx + wx * (fail ? 0.9 : 1.4);
          const ay = dy + wy * (fail ? 0.9 : 1.4);
          const u = ax * hx + ay * hy - 0.85;
          const v = -ax * hy + ay * hx;
          if (u < -0.15) continue;
          const wide = width + (fail ? 0.03 : 0.3) * Math.max(0, u);
          const env = Math.exp(-(v * v) / (wide * wide)) * Math.exp(-Math.max(0, u) / (fail ? 1.9 : 1.2));
          // Feathered to nothing before the tile's edge, so no bloom is cut.
          const edge = smooth(0, 0.14, Math.min(i, j, n - 1 - i, n - 1 - j) / n);
          const o = j * n + i;
          dens[o] = (fail ? smooth(0.35, 0.6, f) : smooth(0.32, 0.8, f)) * env * edge * smooth(-0.15, 0.1, u);
          at[o] = Math.max(0, u) / 2.4 + (f - 0.5) * 0.3;
        }
      }
      const tile = document.createElement("canvas");
      tile.width = n;
      tile.height = n;
      const color = fail ? [142, 27, 27] : [201, 164, 92];
      return { x: g.x - span / 2, y: g.y - span / 2, span, n, dens, at, tile, color, fail, t0: 0 };
    }

    // t in 0..1: the plume reveals outward, then fades to the residue level.
    // `settled` draws the end state for the residue.
    function drawBloom(c, b, t, settled) {
      const grow = settled ? 2 : 1.15 * (1 - Math.pow(1 - Math.min(1, t / 0.75), 3));
      const fade = settled ? (b.fail ? 0.6 : 0.42) : t < 0.55 ? 1 : 1 - ((t - 0.55) / 0.45) * (b.fail ? 0.45 : 0.78);
      const tctx = b.tile.getContext("2d");
      const img = tctx.createImageData(b.n, b.n);
      const px = img.data;
      const [cr, cg, cb] = b.color;
      const gain = b.fail ? 1.8 : 1.5;
      for (let o = 0; o < b.dens.length; o += 1) {
        const d = b.dens[o];
        if (!d) continue;
        const a = Math.min(1, d * smooth(b.at[o] - 0.12, b.at[o], grow) * fade * gain);
        const p = o * 4;
        px[p] = cr;
        px[p + 1] = cg;
        px[p + 2] = cb;
        px[p + 3] = a * 255;
      }
      tctx.putImageData(img, 0, 0);
      c.save();
      // Champagne is light in the ink; red is ink itself, laid over it.
      c.globalCompositeOperation = b.fail ? "source-over" : "lighter";
      c.imageSmoothingEnabled = true;
      c.imageSmoothingQuality = "high";
      c.drawImage(b.tile, b.x, b.y, b.span, b.span);
      c.restore();
    }

    function bake(b) {
      // Older residue thins as new ink settles, so the field never fills.
      rctx.save();
      rctx.globalCompositeOperation = "destination-out";
      rctx.fillStyle = "rgba(0, 0, 0, 0.1)";
      rctx.fillRect(0, 0, W, H);
      rctx.restore();
      drawBloom(rctx, b, 1, true);
    }

    function tick(now) {
      raf = 0;
      const live = [];
      for (const b of blooms) {
        if ((now - b.t0) / BLOOM_MS >= 1) bake(b);
        else live.push(b);
      }
      blooms = live;
      compose();
      for (const b of blooms) drawBloom(ctx, b, (now - b.t0) / BLOOM_MS, false);
      if (blooms.length && !document.hidden) raf = requestAnimationFrame(tick);
    }

    function bloom(fail = false) {
      if (!W) return;
      const b = newBloom(fail);
      if (reduceMotion.matches) {
        bake(b);
        compose();
        return;
      }
      b.t0 = performance.now();
      blooms.push(b);
      if (!raf && !document.hidden) raf = requestAnimationFrame(tick);
    }

    // Residue of blooms that happened before the window showed them.
    function settle(n, failed = 0) {
      for (let i = 0; i < n; i += 1) bake(newBloom(false));
      for (let i = 0; i < failed; i += 1) bake(newBloom(true));
      compose();
    }

    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        if (raf) cancelAnimationFrame(raf);
        raf = 0;
        pausedAt = performance.now();
      } else if (blooms.length) {
        const gap = performance.now() - pausedAt;
        for (const b of blooms) b.t0 += gap;
        raf = requestAnimationFrame(tick);
      }
    });

    // The field is rendered, not drawn per frame, so a window being dragged
    // wider waits for the drag to rest; the canvas stretches meanwhile.
    let pending = 0;
    new ResizeObserver(() => {
      clearTimeout(pending);
      pending = setTimeout(() => requestAnimationFrame(resize), W ? 120 : 0);
    }).observe($("#field"));

    return { bloom, settle, resize };
  })();

  // The room-left arc: the one lit thing on the watch, riding the bezel.
  // It sweeps in the first time a watch is read, and eases after that; a
  // send takes each proven track's room off it as it lands.
  function setRoomArc(st) {
    const arc = $("#room-arc");
    const free = S.roomFree != null ? S.roomFree : st && st.free_bytes;
    const pct =
      st && st.connected && st.capacity_bytes ? Math.max(0, Math.min(100, (free / st.capacity_bytes) * 100)) : 0;
    const value = `${pct.toFixed(2)} 100`;
    if (!arc.classList.contains("live")) {
      if (!pct) {
        arc.style.strokeDasharray = "0 100";
        return;
      }
      arc.style.strokeDasharray = "0 100";
      void arc.getBoundingClientRect();
      arc.classList.add("live");
      requestAnimationFrame(() => {
        arc.style.strokeDasharray = value;
      });
      return;
    }
    arc.style.strokeDasharray = value;
  }

  // The dive bezel turns one click (6°, counterclockwise) per proven track.
  function turnBezel(clicks = 1) {
    S.bezel += clicks;
    $("#watch").style.setProperty("--bz", `${(-6 * S.bezel) % 360}deg`);
  }

  // ------------------------------------------------------------ IPC

  const TAURI = window.__TAURI__;
  // Inside the shell's webview the global must exist; its absence is a
  // broken build, never a reason to fall back to demo data.
  const IN_SHELL = location.protocol === "tauri:" || location.hostname === "tauri.localhost";

  function tauriApi() {
    const invoke = (cmd, args) => TAURI.core.invoke(cmd, args);
    const onProgress = (cb) => TAURI.event.listen("pelican://progress", (e) => cb(e.payload));
    const onBackup = (cb) => TAURI.event.listen("pelican://backup", (e) => cb(e.payload));
    function onDrop(cb) {
      const wv = TAURI.webview && TAURI.webview.getCurrentWebview && TAURI.webview.getCurrentWebview();
      if (wv && wv.onDragDropEvent) {
        wv.onDragDropEvent((e) => {
          const p = e.payload || {};
          if (p.type === "enter" || p.type === "over") cb("over");
          else if (p.type === "leave") cb("leave");
          else if (p.type === "drop") cb("drop", p.paths || []);
        });
        return;
      }
      TAURI.event.listen("tauri://drag-enter", () => cb("over"));
      TAURI.event.listen("tauri://drag-leave", () => cb("leave"));
      TAURI.event.listen("tauri://drag-drop", (e) => cb("drop", (e.payload && e.payload.paths) || []));
    }
    return { invoke, onProgress, onBackup, onDrop };
  }

  let api = null; // set in boot()

  // A watch that stopped answering MTP (it happens after a restart). The
  // core's own words, core error::REPLUG, used everywhere it can surface.
  const REPLUG_WHAT = "The watch isn't answering.";
  const REPLUG_DO = "Unplug it, wait five seconds, plug it back in.";
  const REPLUG = `${REPLUG_WHAT} ${REPLUG_DO}`;
  const isWedged = (s) => /isn['’]t answering/i.test(String(s || ""));

  // What cannot be undone, said the same way everywhere it is said.
  const NO_DELETE =
    "Nothing can be deleted from the watch one song at a time. The only way to clear it is a factory reset, and that erases everything on the watch: activities, health data, settings, Garmin Pay and music.";
  const NO_DELETE_SHORT =
    "What you send stays on the watch. Nothing can be deleted from it one song at a time; only a factory reset clears it, and that erases everything on the watch, not just music.";

  // Errors from the shell arrive as strings; a busy device gets a calm line.
  function errText(e) {
    const s = typeof e === "string" ? e : (e && (e.message || e.error)) || String(e);
    if (/\bbusy\b/i.test(s)) return "The watch is busy with another task. Try again when it finishes.";
    return s;
  }

  // An error line, and what to do about it.
  function errorBlock(err, fallbackFix) {
    if (isWedged(err)) {
      return `<strong>${esc(REPLUG_WHAT)}</strong>${esc(REPLUG_DO)}`;
    }
    return `<strong>${esc(err)}</strong>${esc(fallbackFix)}`;
  }

  // ------------------------------------------------------------ state

  const S = {
    view: "watch",
    status: null,
    places: null,
    place: null, // the path the explorer is rooted at
    showChosen: false, // the explorer shows the chosen list instead of a place
    tree: new Map(), // path -> { state: "loading" | "ok" | "error", l, error }
    open: new Set(), // folders shown open in the explorer
    chosen: [], // [{ path, name, kind: "dir" | "file", count? }], in the order selected
    overrides: { artist: "", album: "", genre: "", year: "" },
    mix: { on: false, name: "" }, // "Playlist" in the window; `mix` in the IPC
    order: null, // explicit per-file order once a playlist is reordered
    resend: false,
    resendSources: new Set(), // per-track "Send again"
    tagsOpen: false, // the run-wide tag fields are shown
    preview: null,
    previewing: false,
    previewError: null,
    previewSeq: 0,
    run: null,
    watchRows: null,
    watchError: null,
    ledger: null,
    ledgerError: null,
    rule: null, // udev_rule_status, read while no watch is reachable
    ruleBusy: false, // the password prompt is open
    ruleInstalled: false, // installed in this session: next step is a replug
    ruleMsg: null, // { text, error } after a canceled or failed install
    bezel: 0, // clicks the dive bezel has turned
    roomFree: null, // free bytes as a send takes them, until status is read again
    reset: null, // the Clear the watch walkthrough, while it is open
    renaming: false, // the name form is open for a watch that has a name
    nameLater: new Set(), // serials whose naming was put off this session
    nameError: null,
  };

  const STEP_OF = { watch: 0, library: 1, playlist: 1, review: 2, send: 3, done: 3 };
  const FLOW = Object.keys(STEP_OF);
  const FLOW_STEPS = [
    ["watch", "Watch"],
    ["library", "Choose"],
    ["review", "Review"],
    ["send", "Send"],
  ];
  const RESET_STEPS = ["Erases", "Back up", "Reset", "Confirm"];
  let flowView = "watch"; // where Back returns to from the watch records
  const running = () => S.run && !S.run.finished && !S.run.error;

  // ------------------------------------------------------------ notice

  let noticeTimer = 0;
  function notice(text, { error = false, word } = {}) {
    const n = $("#notice");
    clearTimeout(noticeTimer);
    n.hidden = false;
    n.className = `notice${error ? " is-error" : ""}`;
    n.innerHTML =
      `${mark(error ? "failed" : "warn")}<span class="word">${esc(word || (error ? "Problem" : "Note"))}</span>` +
      `<span>${withCode(text)}</span><button type="button" class="text-btn" data-act="dismiss">Dismiss</button>`;
    if (!error) noticeTimer = setTimeout(() => (n.hidden = true), 6000);
  }
  const clearNotice = () => {
    $("#notice").hidden = true;
  };

  // ------------------------------------------------------------ motion

  // Title lines reveal with clip-path, so they are laid out once at final
  // width and never re-wrap mid-motion. Web Animations, so a re-render
  // does not replay an entrance.
  const EASE = "cubic-bezier(0.16, 1, 0.3, 1)";
  function enter(sec) {
    if (reduceMotion.matches) return;
    if (sec.hasAttribute("data-card")) {
      const kids = Array.from(sec.children);
      kids.forEach((el, i) => {
        if (el.classList.contains("title")) {
          el.animate(
            [
              { opacity: 0, filter: "blur(6px)", clipPath: "inset(-20% 50% -20% 50%)" },
              { opacity: 1, filter: "blur(0)", clipPath: "inset(-20% -4% -20% -4%)" },
            ],
            { duration: 520, easing: EASE, fill: "backwards" },
          );
        } else {
          el.animate(
            [
              { opacity: 0, transform: "translateY(8px)" },
              { opacity: 1, transform: "none" },
            ],
            { duration: 340, delay: 90 + Math.min(i, 5) * 34, easing: EASE, fill: "backwards" },
          );
        }
      });
      return;
    }
    const head = $(".head", sec);
    if (head) head.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 240, easing: EASE, fill: "backwards" });
    unfold($$(".rows > li, .places li", sec), 60);
  }

  // Rows arriving as a list: a short stagger, capped.
  function unfold(rows, delay = 0) {
    if (reduceMotion.matches) return;
    rows.slice(0, 14).forEach((el, i) => {
      el.animate(
        [
          { opacity: 0, transform: "translateY(-4px)" },
          { opacity: 1, transform: "none" },
        ],
        { duration: 220, delay: delay + i * 16, easing: EASE, fill: "backwards" },
      );
    });
  }

  // ------------------------------------------------------------ views

  const RENDER = {
    watch: renderWatch,
    library: renderLibrary,
    playlist: renderPlaylist,
    review: renderReview,
    send: renderSend,
    done: renderDone,
    onwatch: renderOnWatch,
    ledger: renderLedger,
    reset: renderReset,
  };

  function show(view, { focus = true } = {}) {
    const prev = S.view;
    S.view = view;
    if (FLOW.includes(view)) flowView = view;
    document.body.dataset.view = view;
    for (const sec of $$(".view")) sec.hidden = sec.id !== `v-${view}`;
    const sec = $(`#v-${view}`);
    RENDER[view]();
    if (focus && prev !== view) enter(sec);
    syncChrome();
    if (focus && prev !== view) {
      const h = $(".title", sec);
      if (h) h.focus({ preventScroll: true });
    }
  }

  function renderSteps() {
    const ol = $("#steps");
    const busy = running();
    if (S.view === "reset" && S.reset) {
      const at = S.reset.step === "done" || S.reset.step === "refused" ? 4 : S.reset.step;
      ol.innerHTML = RESET_STEPS.map((name, i) => {
        const n = i + 1;
        const cur = n === at;
        const ok = !cur && n <= S.reset.reached && !S.reset.checking && S.reset.step !== "done";
        return `<li><button type="button" data-rstep="${n}"${cur ? ' aria-current="step"' : ""}${ok ? "" : " disabled"}>${name}</button></li>`;
      }).join("");
      $(".steps-nav").setAttribute("aria-label", "Clear the watch");
    } else {
      const step = STEP_OF[S.view];
      const have = S.chosen.length > 0;
      ol.innerHTML = FLOW_STEPS.map(([name, word], i) => {
        const cur = i === step;
        const ok =
          !busy &&
          !cur &&
          (name === "watch" || name === "library" || (name === "review" && have) || (name === "send" && S.run && S.run.finished));
        return `<li><button type="button" data-step="${name}"${cur ? ' aria-current="step"' : ""}${ok ? "" : " disabled"}>${word}</button></li>`;
      }).join("");
      $(".steps-nav").setAttribute("aria-label", "Steps");
    }
    // One hairline under the current step, traveling between them.
    const rule = $("#steps-rule");
    const cur = $('[aria-current="step"]', ol);
    if (!cur) {
      rule.style.opacity = "0";
      return;
    }
    const nav = $(".steps-nav").getBoundingClientRect();
    const r = cur.getBoundingClientRect();
    const first = rule.style.opacity !== "1";
    if (first) rule.classList.add("still");
    rule.style.opacity = "1";
    rule.style.transform = `translateX(${r.left - nav.left + 2}px) scaleX(${(Math.max(0, r.width - 4) / 100).toFixed(4)})`;
    if (first) requestAnimationFrame(() => rule.classList.remove("still"));
  }

  function syncChrome() {
    const busy = running();
    renderSteps();
    $$(".aside-nav .link").forEach((b) => {
      const cur = b.dataset.go === S.view || (b.dataset.act === "reset-open" && S.view === "reset");
      if (cur) b.setAttribute("aria-current", "page");
      else b.removeAttribute("aria-current");
      b.disabled = busy;
    });
    const connected = S.status && S.status.connected;
    document.body.classList.toggle("no-watch", !connected);
    setRoomArc(S.status);
  }

  // --- Watch -----------------------------------------------------------

  function classify(st) {
    const k = st.error_kind;
    if (k === "wedged" || isWedged(st.error)) return "wedged";
    if (k === "busy" || k === "gvfs" || k === "permission") return k;
    if (k === "not_found" || k === "other") return "none";
    const e = String(st.error || "").toLowerCase();
    if (/\bbusy\b/.test(e)) return "busy";
    if (/gvfs|gio mount/.test(e) && !/no watch|not found|cable/.test(e)) return "gvfs";
    if (/permission|udev|access denied/.test(e) && !/no watch|not found|cable/.test(e)) return "permission";
    return "none";
  }

  function renderWatch() {
    const v = $("#v-watch");
    const st = S.status;
    if (!st) {
      v.innerHTML = `<h1 class="title" id="t-watch" tabindex="-1">Looking for your watch</h1>
        <p class="lede dim">Reading what is plugged in.</p>`;
      return;
    }
    if (!st.connected) {
      const kind = classify(st);
      if (kind === "wedged") {
        v.innerHTML = `<h1 class="title" id="t-watch" tabindex="-1">The watch isn't answering</h1>
          <p class="lede">${esc(REPLUG_DO)}</p>
          <ol class="replug" aria-label="What to do">
            <li><span class="n">1</span>Unplug the cable from the watch</li>
            <li><span class="n">2</span>Wait five seconds</li>
            <li><span class="n">3</span>Plug it back in</li>
          </ol>
          <p class="facts">This can happen after the watch restarts. Once it's plugged back in, Pelican finds it again.</p>
          <div class="actions">
            <button type="button" class="act" data-act="recheck">Check again</button>
            <button type="button" class="text-btn" data-go="library">Choose music meanwhile</button>
          </div>`;
        return;
      }
      const rule = S.rule;
      // The USB rule is offered where it can be the fix: a permission
      // problem, or no watch at all while the rule is not installed.
      const offer =
        (kind === "permission" || kind === "none") && rule && rule.state !== "current" && rule.state !== "unknown";
      const replug = S.ruleInstalled && kind === "permission" && rule && rule.state === "current";
      // Every state says what to do in plain words, as moves a person makes
      // with the watch and this computer. File names, commands and the
      // shell's own sentence stay folded under "What Pelican saw".
      const plain = replug
        ? {
            title: "Unplug and replug the watch",
            lede: "The USB rule is installed. The watch needs to be plugged in again before it takes effect.",
            steps: ["Unplug the cable from the watch", "Wait five seconds", "Plug it back in, then check again"],
          }
        : offer && kind === "permission"
          ? {
              title: "Permission needed",
              lede: "Pelican can see the watch but this computer won't let it open it yet. Install the USB rule once, and it can.",
              steps: [],
            }
          : {
              none: {
                title: "Connect your watch",
                lede: "Plug the watch into this computer with the cable that came with it.",
                steps: [
                  "Use the Garmin cable. Many phone cables only charge and carry no data",
                  "Plug it straight into the computer, not into a hub or a monitor",
                  "Wake the watch: press any button, then check again",
                ],
              },
              gvfs: {
                title: "Release the watch",
                lede: "The file manager has the watch open, and only one program can hold it at a time.",
                steps: [
                  "Close any file manager window showing the watch",
                  "In the file manager's side bar, press the eject mark next to the watch",
                  "Check again",
                ],
              },
              busy: {
                title: "The watch is busy",
                lede: "Another program is using the watch right now.",
                steps: [
                  "Close Garmin Express, the file manager, or any music app that opened the watch",
                  "If that doesn't free it, unplug the watch and plug it back in",
                  "Check again",
                ],
              },
              permission: {
                title: "Permission needed",
                lede: "The watch is plugged in, but this computer won't let Pelican open it.",
                steps: [
                  "Unplug the watch and plug it back in",
                  "If it still refuses, restart the computer once with the watch unplugged",
                  "Check again",
                ],
              },
            }[kind];
      const saw = st.error
        ? `<details class="every runs saw">
            <summary>${mark("open")}What Pelican saw</summary>
            <p class="rule-why">${withCode(st.error)}</p>
          </details>`
        : "";
      const steps = plain.steps.length
        ? `<ol class="howto fix" aria-label="What to do">${plain.steps.map((t) => `<li>${esc(t)}</li>`).join("")}</ol>`
        : "";
      v.innerHTML = `<h1 class="title" id="t-watch" tabindex="-1">${esc(plain.title)}</h1>
        <p class="lede">${esc(plain.lede)}</p>
        ${steps}
        ${offer ? ruleBlock(rule) : ""}
        <div class="actions${offer ? " after-rule" : ""}">
          <button type="button" class="${offer ? "text-btn" : "act"}" data-act="recheck"${S.ruleBusy ? " disabled" : ""}>Check again</button>
          <button type="button" class="text-btn" data-go="library">Choose music meanwhile</button>
        </div>
        ${saw}`;
      return;
    }
    const room = roomFor(st);
    const facts = [
      `${plural(st.music_objects ?? 0, "track")} of ${st.max_objects ?? MAX_OBJECTS} on the watch`,
      `${(st.ledger?.verified ?? 0).toLocaleString("en")} sent by Pelican${st.ledger?.resets ? " since the last reset" : ""}`,
    ];
    if (st.ledger?.failed) facts.push(`${plural(st.ledger.failed, "failed attempt")} in the ledger`);
    // Plain words first; the shell's own sentence (paths, commands) folds
    // under "What Pelican saw", as on the not-connected cards.
    const warn = st.gvfs_warning
      ? `<p class="warn-line">${mark("warn")}<span><span class="word">Warning</span> The file manager also has the watch open. Close it, or press the eject mark next to the watch in its side bar, before you send.</span></p>
        <details class="every runs saw">
          <summary>${mark("open")}What Pelican saw</summary>
          <p class="rule-why">${withCode(st.gvfs_warning)}</p>
        </details>`
      : "";
    // A name the owner gave it leads; the model moves down into the facts.
    // A watch seen for the first time is offered a name once per session.
    if (st.name && st.model) facts.unshift(st.model);
    const naming = S.renaming || (!st.name && st.serial && !S.nameLater.has(st.serial));
    const nameForm = naming
      ? `<form class="name-form watch-name" data-form="watch-name" autocomplete="off">
          <label class="name-label" for="watch-name">${S.renaming ? "Rename this watch" : "Name this watch"}</label>
          <input class="field-input name-input" id="watch-name" name="name" value="${esc(S.renaming ? st.name || "" : "")}" placeholder="Trail watch" spellcheck="false" maxlength="40"${S.nameError ? ' aria-invalid="true" aria-describedby="watch-name-err"' : ""}>
          <button type="submit" class="act">Save</button>
        </form>
        ${S.nameError ? `<p class="why-not" id="watch-name-err">${esc(S.nameError)}</p>` : ""}
        <p class="facts">So you can tell your watches apart. The name is kept on this computer with this watch's ledger; nothing is written to the watch.</p>`
      : "";
    const later = naming
      ? `<button type="button" class="text-btn" data-act="${S.renaming ? "name-cancel" : "name-later"}">${S.renaming ? "Cancel" : "Not now"}</button>`
      : st.serial
        ? `<button type="button" class="text-btn" data-act="name-edit">${st.name ? "Rename" : "Name this watch"}</button>`
        : "";
    v.innerHTML = `<h1 class="title" id="t-watch" tabindex="-1">${esc(st.name || st.model || "Your watch")}</h1>
      <p class="lede"><span class="gold">${esc(bytes(st.free_bytes))} free</span> · room for about ${room.toLocaleString("en")} tracks</p>
      <p class="facts">${esc(facts.join(" · "))}</p>
      ${warn}
      ${nameForm}
      <div class="actions">
        <button type="button" class="act" data-go="library">Choose music</button>
        ${S.chosen.length ? `<button type="button" class="text-btn" data-go="review">Review what is chosen</button>` : ""}
        ${later}
      </div>`;
  }

  function focusNameInput() {
    const input = $("#watch-name");
    if (input && !STILL) requestAnimationFrame(() => input.focus({ preventScroll: true }));
  }

  async function refreshStatus() {
    try {
      S.status = await api.invoke("status");
    } catch (e) {
      const msg = errText(e);
      S.status = { connected: false, error: msg, error_kind: isWedged(msg) ? "wedged" : undefined };
    }
    S.roomFree = null;
    if (!S.status.connected && classify(S.status) !== "wedged") await refreshRule();
    if (S.view === "watch") renderWatch();
    if (S.view === "reset") renderReset();
    syncChrome();
  }

  // --- The USB rule ----------------------------------------------------
  //
  // The shell installs udev/70-garmin-mtp.rules through polkit: one fixed
  // command, the rule compiled in. The window only asks, shows exactly
  // what runs, and says what to do next.

  async function refreshRule() {
    try {
      S.rule = await api.invoke("udev_rule_status");
    } catch (_) {
      S.rule = null;
    }
  }

  function ruleBlock(rule) {
    const busy = S.ruleBusy;
    const older =
      rule.state === "outdated" ? `<p class="rule-note">An older rule is installed at ${esc(rule.path)}; this replaces it.</p>` : "";
    const action = rule.can_install
      ? `<button type="button" class="act" data-act="install-rule"${busy ? ' disabled aria-busy="true"' : ""}>${
          busy ? `${mark("active")}Waiting for your password` : "Install the USB rule"
        }</button>
        <p class="rule-why">Asks for your password once. Writes one file, <code>/etc/udev/rules.d/70-garmin-mtp.rules</code>, so Pelican can reach the watch without root.</p>`
      : `<p class="rule-why">${esc(rule.why_not)}</p>`;
    const said = S.ruleMsg
      ? `<p class="rule-said${S.ruleMsg.error ? " fail" : ""}" role="status">${mark(S.ruleMsg.error ? "failed" : "warn")}<span>${esc(S.ruleMsg.text)}</span></p>`
      : "";
    return `<div class="rule">
        ${older}
        ${action}
        ${said}
        <details class="every runs">
          <summary>${mark("open")}What it runs</summary>
          <p class="runs-label">The rule, written to /etc/udev/rules.d/70-garmin-mtp.rules</p>
          <pre><code>${esc(rule.rule.trimEnd())}</code></pre>
          <p class="runs-label">The command, run once as root through polkit</p>
          <pre><code>${esc(rule.command)}</code></pre>
          <p class="runs-label">Or run it yourself in a terminal</p>
          <pre><code>${esc(rule.manual)}</code></pre>
        </details>
      </div>`;
  }

  async function installRule() {
    if (S.ruleBusy) return;
    S.ruleBusy = true;
    S.ruleMsg = null;
    renderWatch();
    announce("Asking for your password to install the USB rule.");
    let res;
    try {
      res = await api.invoke("install_udev_rule");
    } catch (e) {
      res = { outcome: "failed", message: errText(e) };
    }
    S.ruleBusy = false;
    if (res.outcome === "installed") {
      S.ruleInstalled = true;
      announce("USB rule installed. Checking for the watch.");
      S.status = null;
      renderWatch();
      await refreshStatus();
      if (S.status && !S.status.connected) announce("The watch is still not reachable. Unplug and replug the watch.");
      return;
    }
    S.ruleMsg = { text: res.message, error: res.outcome === "failed" };
    announce(res.message);
    await refreshRule();
    renderWatch();
    const b = $('[data-act="install-rule"]');
    if (b) b.focus({ preventScroll: true });
  }

  // --- Library: the explorer ------------------------------------------
  //
  // Places down the side (Home, Music, the library, network shares,
  // drives, the whole computer), a folder tree that opens in place and
  // loads each folder as it opens, and a checkbox on every folder and song.
  // Nobody types a path.

  const PLACE_MARK = { home: "home", music: "music", network: "network", drive: "drive", library: "library", computer: "computer" };
  const COMPUTER = { label: "Computer", path: "/", kind: "computer" };
  const under = (p, dir) => p.startsWith(dir.endsWith("/") ? dir : `${dir}/`);

  async function loadPlaces() {
    if (S.places) return;
    try {
      S.places = await api.invoke("places");
    } catch (_) {
      S.places = [];
    }
    if (!S.places.some((p) => p.path === "/")) S.places = [...S.places, COMPUTER];
    if (!S.place) {
      const pick =
        S.places.find((p) => p.kind === "library") || S.places.find((p) => p.kind === "music") || S.places[0];
      S.place = pick.path;
    }
  }

  async function loadDir(path) {
    const cur = S.tree.get(path);
    if (cur && (cur.state === "ok" || cur.state === "loading")) return;
    S.tree.set(path, { state: "loading" });
    if (S.view === "library") paintTree();
    try {
      S.tree.set(path, { state: "ok", l: await api.invoke("library_list", { path }) });
    } catch (e) {
      S.tree.set(path, { state: "error", error: errText(e) });
    }
    if (S.view === "library") paintTree(path);
  }

  async function openLibrary() {
    await loadPlaces();
    if (S.view === "library") renderLibrary();
    if (!S.showChosen) await loadDir(S.place);
  }

  // A selected folder covers everything inside it.
  function coveredBy(path) {
    const c = S.chosen.find((x) => x.kind === "dir" && under(path, x.path));
    return c ? c.path : null;
  }
  const isChosen = (path) => S.chosen.some((c) => c.path === path);
  const hasChosenInside = (path) => S.chosen.some((c) => under(c.path, path));

  const dirEntry = (d) => ({ path: d.path, name: d.name, kind: "dir", count: d.audio_files });
  const fileEntry = (f) => ({ path: f.path, name: f.name, kind: "file" });

  function pick(el) {
    const path = el.dataset.path;
    if (el.checked) {
      const entry = el.dataset.kind === "dir" ? dirEntry({ path, name: el.dataset.name, audio_files: Number(el.dataset.count) }) : fileEntry({ path, name: el.dataset.name });
      // A folder takes the place of anything already selected inside it.
      const at = S.chosen.findIndex((c) => under(c.path, path));
      S.chosen = S.chosen.filter((c) => !under(c.path, path));
      if (at >= 0) S.chosen.splice(Math.min(at, S.chosen.length), 0, entry);
      else S.chosen.push(entry);
    } else if (isChosen(path)) {
      S.chosen = S.chosen.filter((c) => c.path !== path);
    } else {
      const anc = coveredBy(path);
      if (anc) uncover(path, anc);
    }
    invalidatePreview();
    paintTree();
    paintBar();
    syncChrome();
  }

  // Clear something inside a selected folder: the folder gives way to
  // everything in it except that one, level by level. Every level is
  // loaded already, since the row was on screen to be cleared.
  function uncover(path, anc) {
    const idx = S.chosen.findIndex((c) => c.path === anc);
    const inserts = [];
    let cur = anc;
    for (;;) {
      const n = S.tree.get(cur);
      if (!n || n.state !== "ok") break;
      const rest = path.slice((cur.endsWith("/") ? cur : `${cur}/`).length).split("/");
      const next = `${cur.replace(/\/$/, "")}/${rest[0]}`;
      for (const d of n.l.dirs) if (d.path !== next && (d.audio_files || d.has_subdirs)) inserts.push(dirEntry(d));
      for (const f of n.l.files) if (f.path !== next) inserts.push(fileEntry(f));
      if (next === path) break;
      cur = next;
    }
    S.chosen.splice(idx, 1, ...inserts);
  }

  function chosenSummary() {
    const dirs = S.chosen.filter((c) => c.kind === "dir").length;
    const files = S.chosen.length - dirs;
    if (!S.chosen.length) return "Select folders or songs. Open a folder with its arrow.";
    const bits = [];
    if (dirs) bits.push(plural(dirs, "folder"));
    if (files) bits.push(plural(files, "song"));
    return `${bits.join(" and ")} chosen`;
  }

  function placeHead() {
    if (S.showChosen) return `<p class="where">Chosen for this send, in the order you selected them.</p>`;
    const p = (S.places || []).find((x) => x.path === S.place);
    return `<p class="where">${p ? `${esc(p.label)} · ` : ""}<span class="path">${esc(S.place || "")}</span></p>`;
  }

  function placesHtml() {
    const list = (S.places || [])
      .map((p) => {
        const cur = !S.showChosen && p.path === S.place;
        return `<li><button type="button" class="place" data-act="place" data-path="${esc(p.path)}" title="${esc(p.path)}"${
          cur ? ' aria-current="location"' : ""
        }>${mark(p.path === "/" ? "computer" : PLACE_MARK[p.kind] || "drive")}<span class="pl">${esc(p.label)}</span></button></li>`;
      })
      .join("");
    const n = S.chosen.length;
    return `<nav class="places" aria-label="Places">
        <ul>${list}</ul>
        <ul class="places-chosen"><li><button type="button" class="place" data-act="chosen-list"${S.showChosen ? ' aria-current="location"' : ""}>${mark("list")}<span class="pl">Chosen</span><span class="n" id="chosen-n">${n ? n.toLocaleString("en") : ""}</span></button></li></ul>
      </nav>`;
  }

  function renderLibrary() {
    const v = $("#v-library");
    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-library" tabindex="-1">Library</h1>
        <div id="lib-where">${placeHead()}</div>
      </div>
      <div class="explorer">
        ${placesHtml()}
        <ul class="rows tree" id="tree" aria-label="Folders and songs"></ul>
      </div>
      <div class="bar" id="lib-bar"></div>`;
    paintTree();
    paintBar();
  }

  function paintBar() {
    const bar = $("#lib-bar");
    if (!bar) return;
    const have = S.chosen.length > 0;
    // The live region stays in place so each change is announced.
    let count = $(".count", bar);
    if (!count) {
      bar.innerHTML = `<span class="count" aria-live="polite"></span>`;
      count = $(".count", bar);
    }
    count.textContent = chosenSummary();
    while (count.nextSibling) count.nextSibling.remove();
    count.insertAdjacentHTML(
      "afterend",
      `${have ? `<button type="button" class="text-btn" data-act="clear-chosen">Clear</button>` : ""}
      <button type="button" class="act" data-act="make-playlist"${have ? "" : " disabled"}>Make a playlist</button>
      <button type="button" class="act" data-act="review-album"${have ? "" : " disabled"}>Review</button>`,
    );
    const n = $("#chosen-n");
    if (n) n.textContent = have ? S.chosen.length.toLocaleString("en") : "";
  }

  // Re-render the tree body only, keeping scroll and focus. `opened` is a
  // folder whose children just arrived: they unfold under it.
  function paintTree(opened) {
    const ul = $("#tree");
    if (!ul) return;
    const active = document.activeElement;
    const activeId = active && ul.contains(active) ? active.id : null;
    const top = ul.scrollTop;
    ul.innerHTML = S.showChosen ? chosenRows() : treeRows(S.place, 0);
    for (const el of $$("[data-mixed]", ul)) el.indeterminate = true;
    for (const li of $$("[data-d]", ul)) li.style.setProperty("--d", li.dataset.d);
    ul.scrollTop = top;
    if (activeId) {
      const el = document.getElementById(activeId);
      if (el) el.focus({ preventScroll: true });
    }
    if (opened && opened !== S.place) unfold($$(`[data-parent="${CSS.escape(opened)}"]`, ul));
    else if (opened === S.place) unfold($$("li", ul));
  }

  function treeRows(path, depth) {
    const n = S.tree.get(path);
    // Depth goes on as a data attribute and becomes --d in paintTree: the
    // CSP (style-src 'self') refuses inline style attributes.
    const pad = `data-d="${depth}" data-parent="${esc(path)}"`;
    if (!n || n.state === "loading") {
      return depth === 0 ? skeletonRows(6) : `<li class="row tree-row is-note" ${pad}><span></span><span></span><span class="t dim">Opening…</span></li>`;
    }
    if (n.state === "error") {
      const msg = isWedged(n.error) ? n.error : `${n.error} If this is a network share, check it is mounted.`;
      return `<li class="row tree-row is-note" ${pad}><span></span><span class="fail">${mark("failed")}</span><span class="t wrap"><span class="fail word">Can't open</span> <span class="dim">${esc(msg)}</span></span></li>`;
    }
    const { dirs, files } = n.l;
    if (!dirs.length && !files.length) {
      return depth === 0
        ? `<li class="empty-row"><p class="empty"><strong>No music here.</strong>Pelican reads MP3, FLAC, WAV, AAC/M4A, Ogg, Opus, WMA, APE and AIFF. Choose another place, or open a folder that holds albums.</p></li>`
        : `<li class="row tree-row is-note" ${pad}><span></span><span></span><span class="t dim">No music in this folder.</span></li>`;
    }
    let out = "";
    for (const d of dirs) {
      const has = d.audio_files > 0 || d.has_subdirs;
      const open = has && S.open.has(d.path);
      const h = hashStr(d.path);
      const on = isChosen(d.path) || !!coveredBy(d.path);
      const mixed = !on && hasChosenInside(d.path);
      const sub =
        d.audio_files > 0
          ? `${plural(d.audio_files, "song")}${d.has_subdirs ? " · folders inside" : ""}`
          : d.has_subdirs
            ? "folders inside"
            : "no music";
      out += `<li class="row tree-row dir${open ? " is-open" : ""}${has ? "" : " is-empty"}" ${pad}>
          <span class="disc">${
            has
              ? `<button type="button" class="chev" tabindex="-1" aria-hidden="true" data-act="toggle" data-path="${esc(d.path)}">${mark("open")}</button>`
              : ""
          }</span>
          <input type="checkbox" class="check" id="c-${h}" data-act="pick" data-kind="dir" data-path="${esc(d.path)}" data-name="${esc(d.name)}" data-count="${d.audio_files}" aria-label="Choose ${esc(d.name)}"${on ? " checked" : ""}${mixed ? " data-mixed" : ""}${has ? "" : " disabled"}>
          ${
            has
              ? `<button type="button" class="t name-btn" id="x-${h}" data-act="toggle" data-path="${esc(d.path)}" aria-expanded="${open}">${esc(d.name)}<span class="sub">${esc(sub)}</span></button>`
              : `<span class="t">${esc(d.name)}<span class="sub">${esc(sub)}</span></span>`
          }
          <span></span>
        </li>`;
      if (open) out += treeRows(d.path, depth + 1);
    }
    for (const f of files) {
      const h = hashStr(f.path);
      const on = isChosen(f.path) || !!coveredBy(f.path);
      out += `<li class="row tree-row file" ${pad}>
          <span class="disc"></span>
          <input type="checkbox" class="check" id="c-${h}" data-act="pick" data-kind="file" data-path="${esc(f.path)}" data-name="${esc(f.name)}"${on ? " checked" : ""}>
          <label class="t" for="c-${h}">${esc(f.name)}</label>
          <span class="size">${esc(bytes(f.bytes))}</span>
        </li>`;
    }
    return out;
  }

  function chosenRows() {
    if (!S.chosen.length) {
      return `<li class="empty-row"><p class="empty"><strong>Nothing chosen yet.</strong>Select folders or songs in any place, or drop them on this window. Whole albums work best: each song is tagged from its file, or from its folder names when the file has none.</p></li>`;
    }
    return S.chosen
      .map(
        (c, i) => `<li class="row chosen-row">
          ${mark(c.kind === "dir" ? "library" : "music")}
          <span class="t">${esc(c.name)}<span class="sub">${esc(dirname(c.path))}</span></span>
          <span class="size">${c.kind === "dir" ? (c.count ? plural(c.count, "song") : "folder") : "song"}</span>
          <button type="button" class="text-btn" data-act="unchoose" data-i="${i}" aria-label="Leave out ${esc(c.name)}">Leave out</button>
        </li>`,
      )
      .join("");
  }

  async function toggleDir(path) {
    if (S.open.has(path)) {
      S.open.delete(path);
      paintTree();
      return;
    }
    S.open.add(path);
    const n = S.tree.get(path);
    if (n && n.state === "ok") paintTree(path);
    else await loadDir(path);
  }

  function addChosen(entries) {
    let added = 0;
    for (const e of entries) {
      if (isChosen(e.path) || coveredBy(e.path)) continue;
      S.chosen = S.chosen.filter((c) => !under(c.path, e.path));
      S.chosen.push(e);
      added += 1;
    }
    if (added) invalidatePreview();
    return added;
  }

  function invalidatePreview() {
    S.preview = null;
    S.order = null;
  }

  function skeletonRows(n) {
    let s = "";
    for (let i = 0; i < n; i += 1) {
      s += `<li class="row skel"><span class="bar-a"></span><span class="bar-b"></span></li>`;
    }
    return s;
  }

  function hashStr(s) {
    let h = 2166136261;
    for (let i = 0; i < s.length; i += 1) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return (h >>> 0).toString(36);
  }

  // --- Playlist: name it, press Enter ----------------------------------

  function songsLabel() {
    let songs = 0;
    let exact = true;
    const folders = new Set();
    for (const c of S.chosen) {
      if (c.kind === "file") {
        songs += 1;
        folders.add(dirname(c.path));
      } else {
        songs += c.count || 0;
        if (!c.count) exact = false;
        folders.add(c.path);
      }
    }
    const from = folders.size > 1 ? ` from ${plural(folders.size, "folder")}` : "";
    return `${exact ? "" : "At least "}${plural(songs, "song")}${from}`;
  }

  function renderPlaylist() {
    const v = $("#v-playlist");
    if (!S.chosen.length) {
      v.innerHTML = `<h1 class="title" id="t-playlist" tabindex="-1">Make a playlist</h1>
        <p class="lede">Select songs in the library first, from as many folders as you like.</p>
        <div class="actions"><button type="button" class="act" data-go="library">Open the library</button></div>`;
      return;
    }
    v.innerHTML = `<h1 class="title" id="t-playlist" tabindex="-1">Name the playlist</h1>
      <p class="lede">${esc(songsLabel())}, in the order you selected them. On the watch it appears under Albums, in this order.</p>
      <form class="name-form" data-form="playlist" autocomplete="off">
        <label class="sr-only" for="pl-name">Playlist name</label>
        <input class="field-input name-input" id="pl-name" name="name" value="${esc(S.mix.name)}" placeholder="Long Run" spellcheck="false" maxlength="80">
        <button type="submit" class="act">Review</button>
      </form>
      <p class="facts">Press Enter to review it. You can change the order there.</p>
      <p class="facts">A song that is already on the watch goes again as a new copy inside the playlist.</p>
      <div class="actions"><button type="button" class="text-btn" data-go="library">Back to the library</button></div>`;
    const input = $("#pl-name");
    if (input && !STILL) requestAnimationFrame(() => input.focus({ preventScroll: true }));
  }

  // --- Review ----------------------------------------------------------

  function previewReq() {
    const o = {};
    for (const k of ["artist", "album", "genre", "year"]) {
      const val = S.overrides[k].trim();
      if (!val) continue;
      if (k === "year" && !/^\d{4}$/.test(val)) continue;
      if (k === "album" && S.mix.on) continue;
      o[k] = val;
    }
    const name = S.mix.name.trim();
    return {
      paths: S.order || S.chosen.map((c) => c.path),
      overrides: o,
      mix: S.mix.on && name ? { name } : null,
      resend: S.resend,
      resend_sources: [...S.resendSources],
    };
  }

  async function runPreview() {
    const seq = (S.previewSeq += 1);
    S.previewing = true;
    S.previewError = null;
    if (S.view === "review") updateReviewBusy();
    try {
      const p = await api.invoke("preview", { req: previewReq() });
      if (seq !== S.previewSeq) return;
      S.preview = p;
    } catch (e) {
      if (seq !== S.previewSeq) return;
      S.previewError = errText(e);
    }
    S.previewing = false;
    if (S.view === "review") renderReview({ keepFocus: true });
  }

  let previewTimer = 0;
  const previewSoon = () => {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(runPreview, 320);
  };

  function updateReviewBusy() {
    const list = $("#v-review .rows");
    if (list) list.setAttribute("aria-busy", "true");
  }

  function verdictCell(f) {
    if (f.verdict === "send") {
      if (S.resendSources.has(f.source)) {
        return `<span class="state s-send">${mark("pending")}Send again</span><span class="why dim">another copy, under a new name</span>
          <button type="button" class="text-btn row-btn" data-act="again-undo" data-src="${esc(f.source)}" aria-label="Keep ${esc(f.title)} skipped">Keep skipped</button>`;
      }
      return `<span class="state s-send">${mark("pending")}Send</span><span class="size">est. ${esc(bytes(f.est_bytes))}</span>`;
    }
    if (f.verdict === "skip") {
      const again = /^already on watch/i.test(f.reason || "")
        ? `<button type="button" class="text-btn row-btn" data-act="again-one" data-src="${esc(f.source)}" aria-label="Send ${esc(f.title)} again">Send again</button>`
        : "";
      return `<span class="state s-skip">${mark("skipped")}Skip</span><span class="why dim">${esc(f.reason || "")}</span>${again}`;
    }
    return `<span class="state s-refused">${mark("failed")}Refused</span><span class="why fail">${esc(f.reason || "")}</span>`;
  }

  function renderReview({ keepFocus = false } = {}) {
    const v = $("#v-review");
    if (!S.chosen.length) {
      v.innerHTML = `<div class="head"><h1 class="title" id="t-review" tabindex="-1">Review</h1></div>
        <div class="rows"><p class="empty"><strong>Nothing to review yet.</strong>Choose albums or songs first; this is where you see how each will be tagged, whether it fits, and what gets sent.</p></div>
        <div class="bar"><button type="button" class="act" data-go="library">Choose music</button></div>`;
      return;
    }
    if (!S.preview && !S.previewing && !S.previewError) {
      runPreview();
    }
    const active = keepFocus ? document.activeElement : null;
    const activeId = active && active.id;
    const sel = active && "selectionStart" in active ? [active.selectionStart, active.selectionEnd] : null;

    const p = S.preview;
    const mixing = S.mix.on;
    let summary = "Reading tags and sizes…";
    let list = `<ul class="rows" aria-busy="true">${skeletonRows(8)}</ul>`;
    if (S.previewError) {
      summary = "The preview could not be built.";
      list = `<div class="rows"><p class="empty"><strong>Pelican could not read what was chosen.</strong>${esc(S.previewError)} Check the folders are still there (a network share may need remounting), then try again.</p>
        <p class="empty"><button type="button" class="act" data-act="preview">Try again</button></p></div>`;
    } else if (p) {
      const t = p.totals;
      const bits = [`${plural(p.files.length, "song")}`];
      bits.push(`${t.send.toLocaleString("en")} to send`);
      if (t.skip) bits.push(`${t.skip.toLocaleString("en")} skipped`);
      if (t.refused) bits.push(`${t.refused.toLocaleString("en")} refused`);
      summary = `${mixing && S.mix.name.trim() ? `Playlist “${S.mix.name.trim()}” · ` : ""}${bits.join(" · ")} · about ${bytes(t.est_bytes)}`;
      list = `<ol class="rows${S.previewing ? " is-stale" : ""}" aria-label="${mixing ? "Playlist, in order" : "Songs"}"${S.previewing ? ' aria-busy="true"' : ""}>${p.files
        .map((f, i) => {
          const sub = [f.artist, f.album, f.year, f.genre].filter(Boolean).join(" · ");
          const order = mixing
            ? `<span class="order">
                <button type="button" data-act="move" data-i="${i}" data-d="-1" aria-label="Move ${esc(f.title)} up"${i === 0 ? " disabled" : ""}><svg class="mk" aria-hidden="true"><use href="#m-up"/></svg></button>
                <button type="button" data-act="move" data-i="${i}" data-d="1" aria-label="Move ${esc(f.title)} down"${i === p.files.length - 1 ? " disabled" : ""}><svg class="mk" aria-hidden="true"><use href="#m-down"/></svg></button>
              </span>`
            : "";
          return `<li class="row track-row is-${esc(f.verdict)}" data-src="${esc(f.source)}">
            <span class="num">${esc(f.track ?? "-")}</span>
            <span class="t">${esc(f.title)}<span class="sub">${esc(sub || "No artist or album")}</span></span>
            <span class="end">${verdictCell(f)}</span>
            ${order}
          </li>`;
        })
        .join("")}</ol>
        <p class="est-note">Sizes are estimates at the one profile (192 kbps MP3); the exact size is known once each song is converted.</p>`;
    }

    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-review" tabindex="-1">${mixing ? "Review the playlist" : "Review"}</h1>
        <p class="quiet">${esc(summary)}</p>
      </div>
      <div class="review-body${mixing ? " mixing" : ""}">
        <div class="review-list">${list}</div>
        <div class="review-side">
          <div class="side-scroll">
          ${fitBlock(p)}
          <div>
            <label class="toggle"><input type="checkbox" class="check" id="mix-on" data-act="mix-toggle"${mixing ? " checked" : ""}>
              <span>Send as a playlist<span class="dim">On the watch it appears under Albums, in this order.</span></span></label>
            ${
              mixing
                ? `<label class="sr-only" for="mix-name">Playlist name</label>
                   <input class="field-input mix-name" id="mix-name" data-input="mix" value="${esc(S.mix.name)}" placeholder="Name the playlist" autocomplete="off" spellcheck="false" maxlength="80">
                   <p class="side-note">Songs already on the watch go again as new copies inside it.</p>`
                : ""
            }
          </div>
          <details class="every tags" id="tags"${S.tagsOpen || Object.values(S.overrides).some((x) => x.trim()) ? " open" : ""}>
            <summary>${mark("open")}Tags for every song</summary>
            <fieldset>
              <legend class="sr-only">Tags for every song</legend>
              <div class="overrides">
                ${overrideInput("artist", "Artist")}
                ${mixing ? "" : overrideInput("album", "Album")}
                ${overrideInput("genre", "Genre")}
                ${overrideInput("year", "Year", 'inputmode="numeric" maxlength="4"')}
              </div>
            </fieldset>
          </details>
          ${
            (p && p.totals.skip) || S.resend
              ? `<label class="toggle"><input type="checkbox" class="check" id="resend" data-act="resend"${S.resend ? " checked" : ""}>
                  <span>Send every skipped song again<span class="dim">Each goes as another copy, under a new name.</span></span></label>`
              : ""
          }
          </div>
          <div class="commit">
            <p class="permanence">${esc(NO_DELETE_SHORT)} <button type="button" class="text-btn inline" data-act="reset-open">How to clear it</button></p>
            ${sendBlock(p)}
          </div>
        </div>
      </div>`;

    if (activeId) {
      const el = document.getElementById(activeId);
      if (el) {
        el.focus({ preventScroll: true });
        if (sel && "setSelectionRange" in el) {
          try {
            el.setSelectionRange(sel[0], sel[1]);
          } catch (_) {
            /* checkboxes have no selection */
          }
        }
      }
    }
  }

  function overrideInput(key, label, extra = "") {
    const val = S.overrides[key];
    const bad = key === "year" && val.trim() && !/^\d{4}$/.test(val.trim());
    const cls = key === "artist" && S.mix.on ? "wide" : "";
    return `<label class="${cls}">${label}
      <input class="field-input" id="ov-${key}" data-input="ov" data-key="${key}" value="${esc(val)}" placeholder="From each file" autocomplete="off" ${extra}${bad ? ' aria-invalid="true"' : ""}></label>`;
  }

  function fitBlock(p) {
    if (!p) return `<div class="fit">${mark("pending")}<span class="word dim">Checking the room</span></div>`;
    const f = p.fits;
    const st = S.status;
    if (!st || !st.connected) {
      return `<div class="fit">${mark("other")}<span class="word">No watch to check against</span>
        <span class="dim">Connect the watch to see whether this fits.</span></div>`;
    }
    const after = f.objects_after ?? (st.music_objects ?? 0) + p.totals.send;
    if (f.ok) {
      const left = (f.free_bytes ?? st.free_bytes) - p.totals.est_bytes;
      return `<div class="fit s-verified">${mark("verified")}<span class="word">Fits</span>
        <span class="dim">About <span class="gold">${esc(bytes(Math.max(0, left)))}</span> will remain · ${after.toLocaleString("en")} of ${st.max_objects ?? MAX_OBJECTS} tracks after</span></div>`;
    }
    return `<div class="fit s-failed">${mark("failed")}<span class="word">Does not fit</span>
      <span class="dim">${esc(f.reason || "")} Leave some songs out and review again, or <button type="button" class="text-btn inline" data-act="reset-open">clear the watch</button>.</span></div>`;
  }

  function sendBlock(p) {
    let why = "";
    const n = p ? p.totals.send : 0;
    if (!S.status || !S.status.connected) why = "Connect the watch to send.";
    else if (!p) why = "";
    else if (S.mix.on && !S.mix.name.trim()) why = "Name the playlist to send it.";
    else if (!p.fits.ok) why = "It does not fit on the watch as chosen.";
    else if (!n) why = "Nothing here needs sending. Use Send again on a song to add another copy.";
    const ok = p && !why && !S.previewing;
    return `<div class="send-block">
      <button type="button" class="act send" data-act="send"${ok ? "" : " disabled"}>${n ? `Send ${plural(n, "song")}` : "Send"}</button>
      ${why ? `<p class="why-not">${esc(why)}</p>` : ""}
    </div>`;
  }

  // --- Send: the credits roll ------------------------------------------

  const LINE_WORD = {
    pending: "Waiting",
    transcoding: "Converting",
    sending: "Sending",
    verified: "Verified",
    skipped: "Skipped",
    failed: "Failed",
  };
  const LINE_MARK = {
    pending: "pending",
    transcoding: "active",
    sending: "active",
    verified: "verified",
    skipped: "skipped",
    failed: "failed",
  };

  function newRun(preview, req) {
    return {
      id: null,
      req,
      name: req.mix ? req.mix.name : null,
      lines: preview.files.map((f) => ({
        title: f.title,
        artist: f.artist,
        est: f.est_bytes || 0,
        state: "pending",
        detail: "",
        remote: null,
        pct: 0,
        shown: false,
        sha: null,
      })),
      current: -1,
      tally: { verified: 0, skipped: 0, failed: 0 },
      stopping: false,
      finished: null,
      error: null,
      buffered: [],
    };
  }

  async function startSend() {
    const p = S.preview;
    if (!p) return;
    const req = previewReq();
    S.run = newRun(p, req);
    show("send");
    try {
      const { run_id: id } = await api.invoke("push", { req });
      S.run.id = id;
      const early = S.run.buffered.filter((e) => e.run_id === id);
      S.run.buffered = [];
      early.forEach(onProgress);
    } catch (e) {
      S.run.error = errText(e);
      renderSend();
      syncChrome();
    }
  }

  function onProgress(ev) {
    const run = S.run;
    if (!run) return;
    if (!run.id) {
      run.buffered.push(ev);
      return;
    }
    if (ev.run_id !== run.id) return;
    const line = run.lines[ev.index];
    switch (ev.kind) {
      case "transcoding":
        run.current = ev.index;
        line.state = "transcoding";
        line.detail = "";
        break;
      case "sending":
        line.state = "sending";
        line.detail = line.remote
          ? `again as ${ev.remote}. The first copy did not read back the same.`
          : `as ${ev.remote}`;
        line.remote = ev.remote;
        line.pct = 0;
        break;
      case "uploading":
        line.pct = ev.total_bytes ? ev.bytes / ev.total_bytes : 0;
        break;
      case "done":
        line.state = ev.outcome;
        line.sha = ev.sha256 || null;
        if (ev.outcome === "verified") {
          line.detail = `sha256 ${(ev.sha256 || "").slice(0, 12)}`;
          run.tally.verified += 1;
          Ink.bloom();
          turnBezel(1);
          if (S.status && S.status.connected) {
            S.roomFree = Math.max(0, (S.roomFree ?? S.status.free_bytes) - line.est);
            setRoomArc(S.status);
          }
          announce(`Verified: ${line.title}`);
        } else if (ev.outcome === "skipped") {
          line.detail = ev.reason || "";
          run.tally.skipped += 1;
        } else {
          line.detail = ev.reason || "";
          run.tally.failed += 1;
          Ink.bloom(true);
          announce(`Failed: ${line.title}. ${ev.reason || ""}`);
        }
        break;
      case "finished":
        run.finished = ev;
        announce(
          `Finished. ${ev.verified} verified, ${ev.skipped} skipped, ${ev.failed} failed${ev.stopped ? ", stopped early" : ""}.`,
        );
        break;
      case "error":
        run.error = ev.message;
        announce(isWedged(ev.message) ? REPLUG : `Send stopped: ${ev.message}`);
        break;
      default:
        return;
    }
    if (line) paintLine(ev.index, ev.kind === "done");
    paintMeta();
    if (ev.kind === "finished") {
      syncChrome();
      refreshStatus();
      S.watchRows = null;
      S.ledger = null;
      setTimeout(() => {
        if (S.view === "send") show("done");
      }, 1400);
    }
    if (ev.kind === "error") {
      renderSend();
      syncChrome();
      if (isWedged(ev.message)) {
        S.status = { connected: false, error_kind: "wedged", error: REPLUG, max_objects: MAX_OBJECTS, ledger: S.status && S.status.ledger };
        syncChrome();
      }
    }
  }

  function lineHtml(l) {
    const meter =
      l.state === "sending" ? `<span class="meter" aria-hidden="true"><i data-w="${l.pct}"></i></span>` : "";
    return `<span class="ct">${esc(l.title)}</span>
      <span class="cs">${mark(LINE_MARK[l.state])}<span class="word">${LINE_WORD[l.state]}</span>${
        l.detail ? `<span class="detail">${esc(l.detail)}</span>` : ""
      }${meter}</span>`;
  }

  // The active credit sits just below the roll's middle.
  const FOLLOW_AT = 0.58;
  // A wheel, touch or key on the roll holds off auto-follow this long.
  const USER_SCROLL_HOLD_MS = 5000;
  let rollTouchedAt = 0;

  function paintLine(i, resolved) {
    const run = S.run;
    const l = run.lines[i];
    const roll = $("#roll-list");
    if (!roll || S.view !== "send") return;
    let li = roll.querySelector(`[data-i="${i}"]`);
    const fresh = !li;
    if (fresh) {
      l.shown = true;
      li = document.createElement("li");
      li.dataset.i = String(i);
      // Keep plan order even when a skip resolves out of turn.
      const after = $$("li", roll).find((x) => Number(x.dataset.i) > i);
      roll.insertBefore(li, after || null);
    }
    li.className = `credit is-${esc(l.state)}${fresh && !reduceMotion.matches ? " rise" : ""}`;
    li.innerHTML = lineHtml(l);
    const bar = li.querySelector(".meter i");
    if (bar) bar.style.transform = `scaleX(${l.pct.toFixed(3)})`;
    if (resolved && !reduceMotion.matches) li.querySelector(".cs").classList.add("resolve");
    follow(li);
  }

  function follow(li) {
    const roll = $("#roll");
    if (!roll || Date.now() - rollTouchedAt < USER_SCROLL_HOLD_MS) return;
    const top = li.offsetTop - roll.clientHeight * FOLLOW_AT + li.offsetHeight / 2;
    roll.scrollTo({ top, behavior: reduceMotion.matches ? "auto" : "smooth" });
    clearRoll();
  }

  // No credit is shown cut in half: one passing under the pinned failure
  // (or out of the roll's top) is hidden whole until it fully clears. An
  // earlier failure under a later one is hidden the same way.
  let rollFrame = 0;
  function clearRoll() {
    if (rollFrame) return;
    rollFrame = requestAnimationFrame(() => {
      rollFrame = 0;
      const roll = $("#roll");
      if (!roll) return;
      const box = roll.getBoundingClientRect();
      const items = $$("li", roll).map((li) => [li, li.getBoundingClientRect()]);
      const stickTop = box.top + parseFloat(getComputedStyle(roll).getPropertyValue("--pin-top") || "0");
      let pin = null;
      for (const [li, r] of items) {
        if (li.classList.contains("is-failed") && r.top <= stickTop + 0.5) pin = [li, r];
      }
      const ceiling = pin ? pin[1].bottom : box.top;
      for (const [li, r] of items) {
        const hide = li !== (pin && pin[0]) && r.top < ceiling - 0.5 && r.bottom > box.top;
        li.classList.toggle("is-under", hide);
      }
    });
  }

  function paintMeta() {
    const run = S.run;
    const m = $("#roll-meta");
    if (!m) return;
    const total = run.lines.length;
    const at = Math.min(total, run.current + 1);
    const t = run.tally;
    const bits = [`${at.toLocaleString("en")} of ${total.toLocaleString("en")}`];
    bits.push(`${t.verified.toLocaleString("en")} verified`);
    if (t.skipped) bits.push(`${t.skipped} skipped`);
    if (t.failed) bits.push(`${t.failed} failed`);
    m.innerHTML = `${esc(bits.join(" · "))}${run.name ? ` · <span class="dim">as the playlist “${esc(run.name)}”</span>` : ""}`;
    const stop = $("#stop");
    if (stop) {
      stop.disabled = run.stopping || !!run.finished || !!run.error;
      stop.textContent = run.stopping ? "Stopping after this song" : "Stop after this song";
    }
  }

  function renderSend() {
    const v = $("#v-send");
    const run = S.run;
    if (!run) {
      v.innerHTML = `<h1 class="title" id="t-send" tabindex="-1">Nothing is sending</h1>
        <p class="lede dim">Choose music and review it first; Send starts from Review.</p>
        <div class="actions"><button type="button" class="act" data-go="library">Choose music</button></div>`;
      return;
    }
    if (run.error && isWedged(run.error)) {
      v.innerHTML = `<h1 class="title" id="t-send" tabindex="-1">The watch stopped answering</h1>
        <p class="lede">${esc(REPLUG_DO)}</p>
        <p class="quiet">${plural(run.tally.verified, "song")} arrived and ${run.tally.verified === 1 ? "was" : "were"} proven before it stopped; ${
          run.tally.verified === 1 ? "it is" : "they are"
        } on the watch and in the ledger. Once the watch is back, review again: what arrived is skipped, and the rest goes.</p>
        <div class="actions"><button type="button" class="act" data-act="back-review">Back to review</button>
        <button type="button" class="text-btn" data-go="watch">Check the watch</button></div>`;
      return;
    }
    if (run.error) {
      v.innerHTML = `<h1 class="title" id="t-send" tabindex="-1">Send interrupted</h1>
        <p class="lede"><span class="state s-failed">${mark("failed")}<span class="word">Failed</span></span> ${withCode(run.error)}</p>
        <p class="quiet">Songs that finished before this are on the watch and in the ledger. Check the cable and the watch, then review again: what is already there will be skipped.</p>
        <div class="actions"><button type="button" class="act" data-act="back-review">Back to review</button>
        <button type="button" class="text-btn" data-go="ledger">Open the ledger</button></div>`;
      return;
    }
    v.innerHTML = `<h1 class="title" id="t-send" tabindex="-1">Sending</h1>
      <p class="roll-meta" id="roll-meta"></p>
      <div class="roll" id="roll" tabindex="0" aria-label="Songs as they are sent">
        <ol class="roll-list" id="roll-list" role="log" aria-live="off"></ol>
      </div>
      <div class="stop-line">
        <button type="button" class="act" id="stop" data-act="stop">Stop after this song</button>
        <span>A song in flight is always finished and proven first.</span>
      </div>`;
    const roll = $("#roll-list");
    roll.innerHTML = run.lines
      .map((l, i) =>
        l.shown || l.state !== "pending" ? `<li class="credit is-${esc(l.state)}" data-i="${i}">${lineHtml(l)}</li>` : "",
      )
      .join("");
    run.lines.forEach((l) => {
      if (l.state !== "pending") l.shown = true;
    });
    for (const li of $$("li", roll)) {
      const bar = li.querySelector(".meter i");
      if (bar) bar.style.transform = `scaleX(${Number(bar.dataset.w).toFixed(3)})`;
    }
    const r = $("#roll");
    const touch = () => {
      rollTouchedAt = Date.now();
    };
    r.addEventListener("wheel", touch, { passive: true });
    r.addEventListener("touchmove", touch, { passive: true });
    r.addEventListener("keydown", touch);
    r.addEventListener("scroll", clearRoll, { passive: true });
    paintMeta();
    const last = roll.lastElementChild;
    if (last) {
      requestAnimationFrame(() => {
        r.scrollTop = last.offsetTop - r.clientHeight * FOLLOW_AT + last.offsetHeight / 2;
        clearRoll();
      });
    }
  }

  // --- Done ------------------------------------------------------------

  function renderDone() {
    const v = $("#v-done");
    const run = S.run;
    if (!run || !run.finished) {
      renderSend();
      return;
    }
    const f = run.finished;
    const title = f.stopped ? "Stopped" : f.verified === 0 && f.failed > 0 ? "Nothing arrived" : "Sent";
    const tally = [
      `<span class="state s-verified">${mark("verified")}${f.verified.toLocaleString("en")} verified</span>`,
      f.skipped ? `<span class="state s-skipped">${mark("skipped")}${f.skipped} skipped</span>` : "",
      f.failed ? `<span class="state s-failed">${mark("failed")}${f.failed} failed</span>` : "",
    ].join("");
    const fails = run.lines.filter((l) => l.state === "failed");
    const words = [];
    if (f.verified) {
      words.push(
        run.name
          ? `Every verified song was read back from the watch and matched by hash. Unplug the watch; the playlist “${run.name}” appears under Albums once it reconnects.`
          : "Every verified song was read back from the watch and matched by hash. Unplug the watch; the music appears in its library once it reconnects.",
      );
    }
    if (f.stopped) words.push("Stopped between songs: the ones not reached were not sent and took no name.");
    if (fails.length) words.push("A failed song can be sent again from Review; it goes under a new name.");
    v.innerHTML = `<h1 class="title" id="t-done" tabindex="-1">${esc(title)}</h1>
      <p class="tally">${tally}</p>
      ${words.map((w) => `<p class="quiet">${esc(w)}</p>`).join("")}
      ${
        fails.length
          ? `<ul class="failures" aria-label="Failed songs">${fails
              .map((l) => `<li>${esc(l.title)}<span class="fail">${mark("failed")} Failed · ${esc(l.detail)}</span></li>`)
              .join("")}</ul>`
          : ""
      }
      <details class="every"><summary>${mark("open")}Every song in this send</summary><ol>${run.lines
        .map(
          (l) =>
            `<li><span class="t">${esc(l.title)}</span><span class="state s-${esc(l.state)}">${mark(LINE_MARK[l.state])}${LINE_WORD[l.state]}${
              l.state === "verified" && l.sha ? ` <span class="dim">${esc(l.sha.slice(0, 8))}</span>` : ""
            }</span></li>`,
        )
        .join("")}</ol></details>
      <div class="actions">
        <button type="button" class="act" data-act="again">Choose more music</button>
        <button type="button" class="text-btn" data-go="onwatch">See what is on the watch</button>
      </div>`;
  }

  // --- On the watch ----------------------------------------------------

  const WATCH_WORD = { ledger: "Sent by Pelican", foreign: "Other", stub: "Broken" };
  const WATCH_MARK = { ledger: "verified", foreign: "other", stub: "failed" };

  async function loadWatchRows() {
    S.watchRows = null;
    S.watchError = null;
    renderOnWatch();
    try {
      S.watchRows = await api.invoke("watch_list");
    } catch (e) {
      S.watchError = errText(e);
    }
    if (S.view === "onwatch") renderOnWatch();
  }

  function renderOnWatch() {
    const v = $("#v-onwatch");
    const rows = S.watchRows;
    let summary = "Reading /Music on the watch…";
    let body = `<ul class="rows" aria-busy="true">${skeletonRows(7)}</ul>`;
    if (S.watchError) {
      summary = "The watch could not be read.";
      body = `<div class="rows"><p class="empty">${errorBlock(S.watchError, "Connect the watch, close anything else that has it open, and check again.")}</p>
        <p class="empty"><button type="button" class="act" data-act="watch-reload">Check again</button></p></div>`;
    } else if (rows && !rows.length) {
      summary = "/Music is empty.";
      body = `<div class="rows"><p class="empty"><strong>Nothing on the watch yet.</strong>What you send appears here, marked as sent by Pelican.</p></div>`;
    } else if (rows) {
      const n = (k) => rows.filter((r) => r.status === k).length;
      const bits = [`${plural(rows.length, "entry", "entries")} in /Music`, `${n("ledger")} sent by Pelican`];
      if (n("foreign")) bits.push(`${n("foreign")} other`);
      if (n("stub")) bits.push(`${n("stub")} broken`);
      summary = bits.join(" · ");
      body = `<ul class="rows" aria-label="Entries on the watch">${rows
        .map((r) => {
          const t = r.title || r.name || "Unnamed entry";
          const sub =
            r.status === "stub"
              ? "Left by a write that never finished; it has no playable file."
              : [r.artist, r.album, r.title && r.name ? r.name : null].filter(Boolean).join(" · ");
          return `<li class="row watch-row">
            <span class="state s-${esc(r.status)}">${mark(WATCH_MARK[r.status] || "other")}${WATCH_WORD[r.status] || esc(r.status)}</span>
            <span class="t">${esc(t)}${sub ? `<span class="sub">${esc(sub)}</span>` : ""}</span>
            <span class="size">${r.bytes != null ? esc(bytes(r.bytes)) : ""}</span>
          </li>`;
        })
        .join("")}</ul>`;
    }
    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-onwatch" tabindex="-1">On the watch</h1>
        <p class="quiet">${esc(summary)}</p>
        <p class="honest">${esc(NO_DELETE)} <button type="button" class="text-btn inline" data-act="reset-open">How to clear the watch</button></p>
      </div>
      ${body}
      <div class="bar"><span class="count">Other: put there by another app or computer.</span>
        <button type="button" class="text-btn" data-act="back-flow">Back</button>
        <button type="button" class="act" data-act="reset-open">Clear the watch</button></div>`;
  }

  // --- Ledger ----------------------------------------------------------

  const EVENT_WORD = { reserve: "Reserved", verified: "Verified", failed: "Failed", reset: "Fresh start" };
  const EVENT_MARK = { reserve: "pending", verified: "verified", failed: "failed", reset: "other" };

  async function loadLedger() {
    S.ledger = null;
    S.ledgerError = null;
    renderLedger();
    try {
      S.ledger = await api.invoke("ledger");
    } catch (e) {
      S.ledgerError = errText(e);
    }
    if (S.view === "ledger") renderLedger();
  }

  function when(at) {
    const d = new Date(at);
    if (Number.isNaN(d.getTime())) return String(at || "");
    return d.toLocaleString("en-US", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  }

  function renderLedger() {
    const v = $("#v-ledger");
    const lg = S.ledger;
    let where = "";
    let body = `<ul class="rows" aria-busy="true">${skeletonRows(7)}</ul>`;
    if (S.ledgerError) {
      body = `<div class="rows"><p class="empty">${errorBlock(S.ledgerError, "")}</p></div>`;
    } else if (lg) {
      where = `<p class="quiet small"><span class="path">${esc(lg.path)}</span> · append-only; a name written here is never used again.</p>`;
      body = lg.rows.length
        ? `<ul class="rows" aria-label="Ledger, newest first">${lg.rows
            .slice()
            .reverse()
            .map((r) => {
              if (r.event === "reset") {
                return `<li class="row ledger-row is-reset">
                  <span class="num">·</span>
                  <span class="state s-reset">${mark("other")}Fresh start</span>
                  <span class="t">Factory reset confirmed<span class="sub">The watch read back empty. Names above this line are new; the numbering keeps rising.</span></span>
                  <span class="when">${esc(when(r.at))}</span>
                </li>`;
              }
              const who = [r.title, r.artist, r.album].filter(Boolean).join(" · ");
              return `<li class="row ledger-row">
                <span class="num">${esc(String(r.counter).padStart(5, "0"))}</span>
                <span class="state s-${esc(r.event)}">${mark(EVENT_MARK[r.event] || "other")}${EVENT_WORD[r.event] || esc(r.event)}</span>
                <span class="t">${esc(r.remote)}<span class="sub">${esc(who)}</span>${r.reason ? `<span class="why fail">${esc(r.reason)}</span>` : ""}</span>
                <span class="when">${esc(when(r.at))}</span>
              </li>`;
            })
            .join("")}</ul>`
        : `<div class="rows"><p class="empty"><strong>No sends from this computer yet.</strong>Before each upload Pelican writes the song's new name here, so no name is ever used twice on this watch.</p></div>`;
    }
    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-ledger" tabindex="-1">Ledger</h1>
        ${where}
      </div>
      ${body}
      <div class="bar"><span class="count">${lg ? `${plural(lg.rows.length, "line")}` : ""}</span>
        <button type="button" class="text-btn" data-act="back-flow">Back</button></div>`;
  }

  // --- Clear the watch: the factory-reset walkthrough ------------------
  //
  // Four title cards: what a reset erases, back up first, the steps
  // on the watch, and the check. Pelican never resets anything itself; it
  // copies the watch's files (read-only) and, at the end, reads /Music and
  // starts a fresh ledger only if the watch really is empty.

  // Reachable from the header, On the watch, Review and a send that does
  // not fit, so Not now returns to wherever it was opened. A walkthrough
  // left mid-way (a backup copying, say) is resumed, not started over.
  function openReset() {
    if (S.reset) {
      if (S.view !== "reset") show("reset");
      return;
    }
    S.reset = {
      from: S.view,
      step: 1,
      reached: 1,
      dest: null,
      backup: null,
      checking: false,
      found: null,
      outcome: null,
      error: null,
    };
    show("reset");
  }

  function resetStep(n) {
    const r = S.reset;
    // No way past the backup while it is still copying, from any control.
    const copying = r.backup && (r.backup.state === "listing" || r.backup.state === "copying");
    if (copying && typeof n === "number" && n > 2) return;
    r.step = n;
    if (typeof n === "number") r.reached = Math.max(r.reached, n);
    r.error = null;
    if (n === 2 && !r.dest) loadBackupDest();
    renderReset();
    const sec = $("#v-reset");
    enter(sec);
    syncChrome();
    const h = $(".title", sec);
    if (h) h.focus({ preventScroll: true });
  }

  async function loadBackupDest() {
    try {
      S.reset.dest = await api.invoke("default_backup_dir");
    } catch (_) {
      S.reset.dest = null;
    }
    if (S.view === "reset" && S.reset.step === 2) paintBackup();
  }

  function watchLine() {
    const st = S.status;
    if (st && st.connected) {
      return `<p class="watch-line">${mark("verified")}<span>${esc(st.model || "Watch")} is connected.</span></p>`;
    }
    if (st && classify(st) === "wedged") {
      return `<p class="watch-line">${mark("warn")}<span>${esc(REPLUG)}</span></p>`;
    }
    return `<p class="watch-line dim">${mark("other")}<span>No watch yet. Plug it in with a data cable.</span></p>`;
  }

  function renderReset() {
    const v = $("#v-reset");
    const r = S.reset;
    if (!r) return;
    const back = (to) => `<button type="button" class="text-btn" data-act="reset-step" data-n="${to}">Back</button>`;
    if (r.step === 1) {
      v.innerHTML = `<h1 class="title" id="t-reset" tabindex="-1">What a reset erases</h1>
        <p class="lede">Nothing can be deleted from the watch one song at a time. The only way to clear it is a factory reset, and that erases everything on it, not just music.</p>
        <ul class="erases" aria-label="What a factory reset erases">
          <li><span class="what">Activities</span><span class="dim">every recorded run and its history</span></li>
          <li><span class="what">Health data</span><span class="dim">sleep, heart rate, steps, stress</span></li>
          <li><span class="what">Settings</span><span class="dim">sport profiles, workouts, your preferences</span></li>
          <li><span class="what">Garmin Pay</span><span class="dim">the wallet; you add your cards again afterward</span></li>
          <li><span class="what">Music</span><span class="dim">every song on the watch</span></li>
        </ul>
        <p class="facts">Pelican can't reset the watch. You do it on the watch, and Pelican checks the result.</p>
        <div class="actions">
          <button type="button" class="act" data-act="reset-step" data-n="2">Back up first</button>
          <button type="button" class="text-btn" data-act="reset-close">Not now</button>
        </div>`;
      return;
    }
    if (r.step === 2) {
      v.innerHTML = `<h1 class="title" id="t-reset" tabindex="-1">Back up first</h1>
        <p class="lede">Sync the watch with Garmin Connect so your activities upload. If you use another fitness app, sync your activities to it first.</p>
        <div class="backup" id="backup"></div>
        <p class="facts">To keep your settings too, back them up to Garmin Connect from the watch. On a Forerunner 165: on the watch face, hold UP, then choose System, Back Up &amp; Restore, Back Up Now.</p>
        <div class="actions" id="backup-next"></div>`;
      paintBackup();
      return;
    }
    if (r.step === 3) {
      v.innerHTML = `<h1 class="title" id="t-reset" tabindex="-1">Reset the watch</h1>
        <p class="lede">On the watch itself. It takes a minute, and the watch does the rest. These are the steps on a Forerunner 165; for other models, see Garmin's manual for your watch.</p>
        <ol class="howto" aria-label="Steps on a Forerunner 165">
          <li>From the watch face, hold <span class="key">UP</span>.</li>
          <li>Select <b>System</b>.</li>
          <li>Select <b>Reset</b>.</li>
          <li>Select <b>Delete Data and Reset Settings</b>.<span class="dim">Not Reset Default Settings: that one keeps the music.</span></li>
          <li>Confirm with the checkmark: press <span class="key">START</span>.</li>
          <li>Wait while the watch erases and restarts. It's done when it asks you to choose a language.</li>
        </ol>
        <p class="facts">On the Forerunner 165, UP is the middle button on the left and START is the top button on the right.</p>
        <div class="actions">
          <button type="button" class="act" data-act="reset-step" data-n="4">It has restarted</button>
          ${back(2)}
        </div>`;
      return;
    }
    if (r.step === 4) {
      const busy = r.checking;
      v.innerHTML = `<h1 class="title" id="t-reset" tabindex="-1">Plug it back in</h1>
        <p class="lede">Plug the watch in once it shows its setup screen. Pelican reads the watch's music folder itself, and starts a fresh record for this watch only if it is empty.</p>
        ${watchLine()}
        ${r.error ? `<p class="empty-line">${errorBlock(r.error, "Plug the watch in, then try again.")}</p>` : ""}
        <div class="actions">
          <button type="button" class="act" data-act="reset-confirm"${busy ? ' disabled aria-busy="true"' : ""}>${
            busy ? `${mark("active")}Reading the music folder` : "The watch is clean"
          }</button>
          ${busy ? "" : back(3)}
        </div>
        <p class="facts">Pair the watch in Garmin Connect again whenever you're ready.</p>`;
      return;
    }
    if (r.step === "refused") {
      const n = r.found ?? 0;
      v.innerHTML = `<h1 class="title" id="t-reset" tabindex="-1">Music is still on the watch</h1>
        <p class="lede"><span class="state s-refused">${mark("failed")}<span class="word">Refused</span></span> ${
          r.message
            ? esc(r.message)
            : `The watch still has ${plural(n, "song file")} in its music folder, so it hasn't been factory-reset. Pelican's record for it is unchanged.`
        }</p>
        <p class="quiet">Check that you chose Delete Data and Reset Settings, not Reset Default Settings: only the first one clears the music.</p>
        <div class="actions">
          <button type="button" class="act" data-act="reset-confirm">Check again</button>
          <button type="button" class="text-btn" data-act="reset-step" data-n="3">See the steps</button>
        </div>`;
      return;
    }
    if (r.step === "done") {
      const o = r.outcome || {};
      v.innerHTML = `<h1 class="title" id="t-reset" tabindex="-1">A clean watch</h1>
        <p class="tally"><span class="state s-verified">${mark("verified")}Verified clean · no songs in its music folder</span></p>
        <p class="lede">${esc(o.message || "The watch is clean. Pelican has started a fresh record for it, so every song can be sent again.")}</p>
        <p class="quiet">The ledger keeps its history; new names keep counting up from where they were.</p>
        <div class="actions">
          <button type="button" class="act" data-act="reset-done">Choose music</button>
          <button type="button" class="text-btn" data-go="ledger">Open the ledger</button>
        </div>`;
    }
  }

  // The backup block on step 2: the offer, the copy in progress, or what
  // it copied and where.
  function paintBackup() {
    const box = $("#backup");
    if (!box) return;
    const r = S.reset;
    const b = r.backup;
    paintBackupNext(b);
    const st = S.status;
    const connected = st && st.connected;
    const where = r.dest ? `<span class="path">${esc(r.dest)}</span>` : "a dated folder in Documents";
    if (!b || b.state === "error") {
      box.innerHTML = `<button type="button" class="act" data-act="backup"${connected ? "" : " disabled"}>Back up the watch's activity files</button>
        <p class="rule-why">Copies the watch's GARMIN folder, where your activities, sleep, health monitoring, records and settings live, to ${where}. Read-only: nothing on the watch changes.</p>
        ${connected ? "" : `<p class="why-not">Connect the watch to back it up.</p>`}
        ${
          b && b.state === "error"
            ? `<p class="rule-said fail" role="status">${mark("failed")}<span>${
                isWedged(b.message) ? esc(REPLUG) : `<span class="word">Not copied</span> ${esc(b.message)}`
              }</span></p>`
            : ""
        }`;
      return;
    }
    if (b.state === "finished") {
      const failed = b.failed && b.failed.length ? ` ${plural(b.failed.length, "file")} could not be read.` : "";
      const blank = b.unreadable ? ` ${plural(b.unreadable, "entry", "entries")} had no name to copy.` : "";
      const cut = b.stopped ? " Stopped before the end: back up again for a full copy." : "";
      box.innerHTML = `<p class="backup-line">${mark(b.stopped || failed ? "skipped" : "verified")}<span><span class="word">${b.stopped ? "Stopped" : "Copied"}</span> ${plural(b.files, "file")} · ${esc(bytes(b.bytes))}</span></p>
        <p class="rule-why">In <span class="path">${esc(b.dest || r.dest || "")}</span>.${esc(failed + blank + cut)}</p>
        ${b.failed && b.failed.length ? `<ul class="backup-failed">${b.failed.slice(0, 5).map((f) => `<li><span class="path">${esc(f.path)}</span> ${esc(f.reason)}</li>`).join("")}</ul>` : ""}`;
      return;
    }
    const pct = b.total ? (b.index + 1) / b.total : 0;
    box.innerHTML = `<p class="backup-line" aria-live="polite">${mark("active")}<span><span class="word">${b.total ? "Copying" : "Listing"}</span> ${
      b.total ? `${(b.index + 1).toLocaleString("en")} of ${b.total.toLocaleString("en")}` : "the watch's folders"
    }</span></p>
      <span class="meter wide" aria-hidden="true"><i></i></span>
      <p class="rule-why"><span class="path">${esc(b.path || "")}</span></p>`;
    $(".meter i", box).style.transform = `scaleX(${pct.toFixed(3)})`;
  }

  // The way on from step 2. The next card says to erase the watch, so a
  // backup in progress holds Continue: it opens when the copy finishes (or
  // was never started). While it runs, the only other move is to stop it,
  // and a stopped or partial copy is named on the button that goes on.
  function paintBackupNext(b) {
    const next = $("#backup-next");
    if (!next) return;
    const back = `<button type="button" class="text-btn" data-act="reset-step" data-n="1">Back</button>`;
    const running = b && (b.state === "listing" || b.state === "copying");
    if (running) {
      next.innerHTML = `<button type="button" class="act" disabled aria-describedby="backup-wait">Continue</button>
        <button type="button" class="text-btn" data-act="backup-stop"${b.stopping ? " disabled" : ""}>${
          b.stopping ? "Stopping after this file" : "Stop the backup"
        }</button>
        <p class="why-not" id="backup-wait">Continue opens when the backup finishes. The next step erases the watch.</p>`;
      return;
    }
    const partial = b && b.state === "finished" && (b.stopped || (b.failed && b.failed.length));
    next.innerHTML = `<button type="button" class="act" data-act="reset-step" data-n="3">${
      partial ? "Continue without a full backup" : "Continue"
    }</button>
      ${back}`;
  }

  async function startBackup() {
    const r = S.reset;
    if (!r || (r.backup && (r.backup.state === "listing" || r.backup.state === "copying"))) return;
    if (!r.dest) await loadBackupDest();
    r.backup = { id: null, state: "listing", index: 0, total: 0, path: "", buffered: [] };
    paintBackup();
    announce("Backing up the watch's files.");
    try {
      const { run_id: id } = await api.invoke("backup_watch", { dest: r.dest });
      r.backup.id = id;
      const early = r.backup.buffered.filter((e) => e.run_id === id);
      r.backup.buffered = [];
      early.forEach(onBackup);
    } catch (e) {
      r.backup = { state: "error", message: errText(e) };
      paintBackup();
    }
  }

  function onBackup(ev) {
    const b = S.reset && S.reset.backup;
    if (!b) return;
    if (!b.id) {
      b.buffered.push(ev);
      return;
    }
    if (ev.run_id !== b.id) return;
    if (ev.kind === "listing") b.state = "listing";
    else if (ev.kind === "file") {
      b.state = "copying";
      b.index = ev.index;
      b.total = ev.total;
      b.path = ev.path;
    } else if (ev.kind === "finished") {
      Object.assign(b, { state: "finished", files: ev.files, bytes: ev.bytes, dest: ev.dest, failed: ev.failed || [], unreadable: ev.unreadable || 0, stopped: !!ev.stopped });
      announce(`Backup finished: ${ev.files} files copied.`);
    } else if (ev.kind === "error") {
      Object.assign(b, { state: "error", message: ev.message });
      announce(isWedged(ev.message) ? REPLUG : `Backup stopped: ${ev.message}`);
    }
    if (S.view === "reset" && S.reset.step === 2) paintBackup();
  }

  async function confirmClean() {
    const r = S.reset;
    if (r.checking) return;
    r.checking = true;
    r.error = null;
    r.step = 4;
    renderReset();
    syncChrome();
    announce("Reading the watch's music folder.");
    let check;
    try {
      check = await api.invoke("reset_check");
    } catch (e) {
      r.checking = false;
      r.error = errText(e);
      renderReset();
      syncChrome();
      return;
    }
    if (!check.clean) {
      r.checking = false;
      r.found = check.audio_objects;
      r.message = null;
      announce(`Refused: the watch still has ${check.audio_objects} song files.`);
      resetStep("refused");
      return;
    }
    let out;
    try {
      out = await api.invoke("reset_ledger");
    } catch (e) {
      r.checking = false;
      r.error = errText(e);
      renderReset();
      syncChrome();
      return;
    }
    r.checking = false;
    if (!out.clean) {
      r.found = out.audio_objects ?? null;
      r.message = out.message;
      resetStep("refused");
      return;
    }
    r.outcome = out;
    S.ledger = null;
    S.watchRows = null;
    announce(out.message);
    resetStep("done");
    Ink.bloom();
    refreshStatus();
  }

  // ------------------------------------------------------------ events

  function go(view) {
    if (running() && view !== "send") return;
    clearNotice();
    if (view === "library") {
      show(view);
      openLibrary();
      return;
    }
    if (view === "onwatch") {
      show(view);
      loadWatchRows();
      return;
    }
    if (view === "ledger") {
      show(view);
      loadLedger();
      return;
    }
    if (view === "review" && !S.preview) runPreview();
    show(view);
  }

  document.addEventListener("click", async (e) => {
    const goBtn = e.target.closest("[data-go]");
    if (goBtn && !goBtn.disabled) {
      go(goBtn.dataset.go);
      return;
    }
    const step = e.target.closest("[data-step]");
    if (step && !step.disabled) {
      const s = step.dataset.step;
      go(s === "send" ? "done" : s);
      return;
    }
    const rstep = e.target.closest("[data-rstep]");
    if (rstep && !rstep.disabled) {
      resetStep(Number(rstep.dataset.rstep));
      return;
    }
    const el = e.target.closest("[data-act]");
    if (!el || el.disabled) return;
    const act = el.dataset.act;
    switch (act) {
      case "dismiss":
        clearNotice();
        break;
      case "install-rule":
        installRule();
        break;
      case "recheck":
        S.ruleMsg = null;
        S.status = null;
        renderWatch();
        await refreshStatus();
        break;
      case "place":
        S.showChosen = false;
        S.place = el.dataset.path;
        $("#lib-where").innerHTML = placeHead();
        $$("#v-library .place").forEach((b) => b.toggleAttribute("aria-current", b === el));
        el.setAttribute("aria-current", "location");
        paintTree(S.place);
        loadDir(S.place);
        break;
      case "chosen-list":
        S.showChosen = true;
        $("#lib-where").innerHTML = placeHead();
        $$("#v-library .place").forEach((b) => b.removeAttribute("aria-current"));
        el.setAttribute("aria-current", "location");
        paintTree(S.place);
        unfold($$("#tree li"));
        break;
      case "toggle":
        toggleDir(el.dataset.path);
        break;
      case "unchoose":
        S.chosen.splice(Number(el.dataset.i), 1);
        invalidatePreview();
        paintTree();
        paintBar();
        syncChrome();
        break;
      case "clear-chosen":
        S.chosen = [];
        invalidatePreview();
        paintTree();
        paintBar();
        syncChrome();
        break;
      case "make-playlist":
        S.mix.on = true;
        invalidatePreview();
        go("playlist");
        break;
      case "review-album":
        if (!S.mix.name.trim()) S.mix.on = false;
        go("review");
        break;
      case "preview":
        runPreview();
        renderReview();
        break;
      case "again-one":
        S.resendSources.add(el.dataset.src);
        runPreview();
        break;
      case "again-undo":
        S.resendSources.delete(el.dataset.src);
        runPreview();
        break;
      case "move": {
        const p = S.preview;
        const i = Number(el.dataset.i);
        const j = i + Number(el.dataset.d);
        if (!p || j < 0 || j >= p.files.length) break;
        const files = p.files.slice();
        [files[i], files[j]] = [files[j], files[i]];
        files.forEach((f, k) => {
          f.track = String(k + 1);
        });
        S.preview = { ...p, files };
        S.order = files.map((f) => f.source);
        renderReview();
        const moved = $(`#v-review .track-row:nth-child(${j + 1})`);
        if (moved && !reduceMotion.matches) {
          moved.animate(
            [
              { transform: `translateY(${(i - j) * 100}%)`, background: "rgb(237 232 222 / 0.05)" },
              { transform: "none", background: "transparent" },
            ],
            { duration: 220, easing: EASE },
          );
        }
        const again = $(`#v-review [data-act="move"][data-i="${j}"][data-d="${el.dataset.d}"]`);
        (again && !again.disabled ? again : $(`#v-review [data-act="move"][data-i="${j}"]`))?.focus();
        previewSoon();
        break;
      }
      case "send":
        startSend();
        break;
      case "stop":
        if (!S.run || S.run.stopping) break;
        S.run.stopping = true;
        paintMeta();
        try {
          await api.invoke("stop");
        } catch (err) {
          notice(errText(err), { error: true });
        }
        break;
      case "back-review":
        S.run = null;
        go("review");
        runPreview();
        break;
      case "again":
        S.run = null;
        S.chosen = [];
        invalidatePreview();
        S.mix = { on: false, name: "" };
        S.overrides = { artist: "", album: "", genre: "", year: "" };
        S.resend = false;
        S.resendSources = new Set();
        go("library");
        break;
      case "watch-reload":
        loadWatchRows();
        break;
      case "back-flow":
        go(flowView);
        break;
      case "reset-open":
        openReset();
        break;
      case "name-edit":
        S.renaming = !!(S.status && S.status.name);
        if (!S.renaming && S.status && S.status.serial) S.nameLater.delete(S.status.serial);
        S.nameError = null;
        renderWatch();
        focusNameInput();
        break;
      case "name-later":
        if (S.status && S.status.serial) S.nameLater.add(S.status.serial);
        S.nameError = null;
        renderWatch();
        break;
      case "name-cancel":
        S.renaming = false;
        S.nameError = null;
        renderWatch();
        break;
      case "reset-step":
        resetStep(Number(el.dataset.n));
        break;
      case "reset-close": {
        const from = (S.reset && S.reset.from) || "onwatch";
        S.reset = null;
        go(from);
        break;
      }
      case "reset-confirm":
        confirmClean();
        break;
      case "reset-done":
        S.reset = null;
        S.resendSources = new Set();
        invalidatePreview();
        go("library");
        break;
      case "backup":
        startBackup();
        break;
      case "backup-stop": {
        const b = S.reset && S.reset.backup;
        if (!b || b.stopping) break;
        b.stopping = true;
        paintBackup();
        announce("Stopping the backup after this file.");
        try {
          await api.invoke("stop");
        } catch (err) {
          notice(errText(err), { error: true });
        }
        break;
      }
      default:
        break;
    }
  });

  document.addEventListener("change", (e) => {
    const el = e.target;
    const act = el.dataset.act;
    if (act === "pick") {
      pick(el);
    } else if (act === "mix-toggle") {
      S.mix.on = el.checked;
      S.order = S.mix.on && S.preview ? S.preview.files.map((f) => f.source) : null;
      renderReview({ keepFocus: true });
      runPreview();
    } else if (act === "resend") {
      S.resend = el.checked;
      runPreview();
    }
  });

  document.addEventListener("input", (e) => {
    const el = e.target;
    if (el.dataset.input === "ov") {
      const key = el.dataset.key;
      S.overrides[key] = el.value;
      if (key === "year") {
        const bad = el.value.trim() && !/^\d{4}$/.test(el.value.trim());
        if (bad) el.setAttribute("aria-invalid", "true");
        else el.removeAttribute("aria-invalid");
      }
      previewSoon();
    } else if (el.dataset.input === "mix") {
      S.mix.name = el.value;
      previewSoon();
    }
  });

  document.addEventListener("submit", async (e) => {
    const named = e.target.closest("[data-form='watch-name']");
    if (named) {
      e.preventDefault();
      const input = named.elements.name;
      try {
        const name = await api.invoke("name_watch", { name: input.value });
        if (S.status) S.status.name = name;
        S.renaming = false;
        S.nameError = null;
        renderWatch();
        announce(`Named ${name}.`);
      } catch (err) {
        S.nameError = errText(err);
        renderWatch();
        focusNameInput();
      }
      return;
    }
    const form = e.target.closest("[data-form='playlist']");
    if (!form) return;
    e.preventDefault();
    const input = form.elements.name;
    const name = input.value.trim();
    if (!name) {
      input.setAttribute("aria-invalid", "true");
      input.focus();
      announce("Name the playlist first.");
      return;
    }
    input.removeAttribute("aria-invalid");
    S.mix = { on: true, name };
    invalidatePreview();
    go("review");
  });

  document.addEventListener(
    "toggle",
    (e) => {
      if (e.target.id === "tags") S.tagsOpen = e.target.open;
    },
    true,
  );

  // The tree's keyboard: Right opens a folder, Left closes it, or on a
  // closed folder or a song moves to the parent folder's row.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    const li = e.target.closest && e.target.closest("#tree .tree-row");
    if (!li) return;
    const b = e.target.closest(".name-btn");
    const open = b && S.open.has(b.dataset.path);
    if (e.key === "ArrowRight") {
      if (!b || open) return;
      toggleDir(b.dataset.path);
    } else if (open) {
      toggleDir(b.dataset.path);
    } else {
      const parent = li.dataset.parent;
      const up = parent && parent !== S.place && document.getElementById(`x-${hashStr(parent)}`);
      if (!up) return;
      up.focus();
    }
    e.preventDefault();
  });

  window.addEventListener("focus", () => {
    if ((S.view === "watch" || S.view === "reset") && S.status && !S.status.connected && !running()) refreshStatus();
  });

  // The ink breathes only while a watch is connected and the window shows.
  document.addEventListener("visibilitychange", () => {
    document.body.classList.toggle("is-hidden", document.hidden);
  });

  // The extensions core transcode::is_audio accepts. A drop carries only
  // paths, so anything else is taken for a folder.
  const AUDIO_EXT = /\.(mp3|m4a|m4b|aac|wav|flac|ogg|oga|opus|wma|ape|aiff|aif|aifc|wv|alac)$/i;

  // Drops: the shell hands absolute paths; a browser (demo) cannot.
  function onDrop(kind, paths) {
    if (running()) return;
    if (kind === "over") document.body.classList.add("dragging");
    else if (kind === "leave") document.body.classList.remove("dragging");
    else if (kind === "drop") {
      document.body.classList.remove("dragging");
      const added = addChosen(
        paths.map((p) => ({ path: p, name: basename(p), kind: AUDIO_EXT.test(p) ? "file" : "dir" })),
      );
      S.showChosen = true;
      if (S.view !== "library") go("library");
      else renderLibrary();
      syncChrome();
      if (!added && paths.length) notice("Already chosen.", { word: "Note" });
    }
  }

  window.addEventListener("resize", () => renderSteps());

  // ------------------------------------------------------------ boot

  // Opened in a plain browser, the window loads demo.js and runs on its
  // fixtures instead of IPC. `script-src 'self'` allows it.
  function loadDemo() {
    return new Promise((resolve, reject) => {
      const el = document.createElement("script");
      el.src = "demo.js";
      el.onload = () =>
        resolve(
          window.PelicanDemo({
            S,
            Ink,
            MAX_OBJECTS,
            RESERVE_BYTES,
            REPLUG,
            basename,
            bytes,
            confirmClean,
            dirname,
            loadLedger,
            loadPlaces,
            loadWatchRows,
            newRun,
            notice,
            previewReq,
            show,
            startBackup,
            syncChrome,
            turnBezel,
          }),
        );
      el.onerror = () => reject(new Error("demo.js did not load"));
      document.head.append(el);
    });
  }

  async function boot() {
    let demo = null;
    if (TAURI) {
      api = tauriApi();
    } else if (IN_SHELL) {
      show("watch", { focus: false });
      notice("The window cannot reach Pelican (window.__TAURI__ is missing). This is a broken build; please report it.", { error: true });
      return;
    } else {
      demo = await loadDemo();
      api = demo.api;
    }
    api.onDrop(onDrop);
    api.onProgress(onProgress);
    api.onBackup(onBackup);
    if (demo) {
      $("#demo-flag").hidden = false;
      await demo.state(location.hash.slice(1) || "watch");
      window.addEventListener("hashchange", () => demo.state(location.hash.slice(1) || "watch"));
      return;
    }
    show("watch", { focus: false });
    await refreshStatus();
  }

  boot();
})();
