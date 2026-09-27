/* Pelican — the window.
 *
 * Plain JS, no build. Talks to the Tauri shell through the IPC contract
 * (status, library_root, set_library_root, library_list, preview, push,
 * stop, watch_list, ledger, and "pelican://progress" events). When
 * window.__TAURI__ is absent the same contract is served by an in-file mock
 * with the owner's real library, clearly marked "Demo data", and the URL
 * hash jumps to each state (#watch, #nowatch, #choose, #library, #review,
 * #review-mix, #send, #done, #onwatch, #ledger).
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
    if (n == null) return "–";
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

  // The watch holds about this many more tracks: free space at the one
  // profile for a 3.5-minute track, capped by the 500-object limit. It is an
  // "about" by design; Review shows the real estimate for what was chosen.
  const RESERVE_BYTES = 2 * 1024 * 1024;
  const AVG_TRACK_BYTES = (210 * 192000) / 8;
  function roomFor(st) {
    const bySpace = Math.floor(Math.max(0, st.free_bytes - RESERVE_BYTES) / AVG_TRACK_BYTES);
    const bySlots = Math.max(0, (st.max_objects ?? 500) - (st.music_objects ?? 0));
    const n = Math.min(bySpace, bySlots);
    return n >= 50 ? Math.floor(n / 10) * 10 : n;
  }

  // ------------------------------------------------------------ the ink

  // A bounded canvas behind the watch. It draws a settled field once and
  // then does nothing: no loop runs at rest. Each verified track blooms one
  // thread of champagne ink out from under the case, which settles into a
  // faint residue. Reduced motion bakes the residue without animating; a
  // hidden window pauses the bloom and resumes it where it was.
  const Ink = (() => {
    const cv = $("#ink");
    const ctx = cv.getContext("2d");
    const base = document.createElement("canvas");
    const bctx = base.getContext("2d");
    const resid = document.createElement("canvas");
    const rctx = resid.getContext("2d");
    const BLOOM_MS = 2800;
    let W = 0;
    let H = 0;
    let dpr = 1;
    let raf = 0;
    let pausedAt = 0;
    let blooms = [];
    let seed = 7;

    // Deterministic, so the settled ink is the same field at every launch.
    const seeded = () => {
      seed = (seed * 16807) % 2147483647;
      return (seed - 1) / 2147483646;
    };

    function geometry() {
      const f = cv.getBoundingClientRect();
      const w = $("#watch").getBoundingClientRect();
      const s = (w.width / 320) * dpr;
      return {
        x: (w.left - f.left) * dpr + 160 * s,
        y: (w.top - f.top) * dpr + 240 * s,
        r: 120 * s,
      };
    }

    function drawBase() {
      seed = 7;
      bctx.fillStyle = "#050506";
      bctx.fillRect(0, 0, W, H);
      const g = geometry();
      for (let i = 0; i < 9; i += 1) {
        const a = seeded() * Math.PI * 2;
        const d = g.r * (0.4 + seeded() * 1.6);
        const x = g.x + Math.cos(a) * d;
        const y = g.y + Math.sin(a) * d * 1.3;
        const rad = g.r * (0.9 + seeded() * 1.8);
        const grad = bctx.createRadialGradient(x, y, 0, x, y, rad);
        const alpha = 0.35 + seeded() * 0.35;
        grad.addColorStop(0, `rgba(22, 21, 25, ${alpha})`);
        grad.addColorStop(0.55, `rgba(16, 16, 19, ${alpha * 0.45})`);
        grad.addColorStop(1, "rgba(5, 5, 6, 0)");
        bctx.fillStyle = grad;
        bctx.fillRect(0, 0, W, H);
      }
      // Fine grain so the ink is a material, not a flat fill.
      const tile = document.createElement("canvas");
      tile.width = 96;
      tile.height = 96;
      const tctx = tile.getContext("2d");
      const img = tctx.createImageData(96, 96);
      for (let i = 0; i < img.data.length; i += 4) {
        const v = seeded() > 0.5 ? 237 : 0;
        img.data[i] = v;
        img.data[i + 1] = v;
        img.data[i + 2] = v;
        img.data[i + 3] = seeded() * 9;
      }
      tctx.putImageData(img, 0, 0);
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

    function newBloom() {
      const g = geometry();
      const count = 3 + Math.floor(Math.random() * 3);
      const heading = Math.random() * Math.PI * 2;
      const fil = [];
      for (let i = 0; i < count; i += 1) {
        let a = heading + (Math.random() - 0.5) * 1.6;
        let x = g.x + Math.cos(a) * g.r * 0.92;
        let y = g.y + Math.sin(a) * g.r * 0.92;
        const pts = [[x, y]];
        const phase = Math.random() * 10;
        const v0 = g.r * (0.018 + Math.random() * 0.016);
        for (let k = 1; k < 110; k += 1) {
          a += Math.sin(k * 0.09 + phase) * 0.045 + (Math.random() - 0.5) * 0.06;
          const v = v0 * Math.exp(-k / 70);
          x += Math.cos(a) * v;
          y += Math.sin(a) * v;
          pts.push([x, y]);
        }
        fil.push({ pts, w: 0.6 + Math.random() * 1.1, a: 0.28 + Math.random() * 0.3 });
      }
      return { g, fil, t0: 0 };
    }

    // t in 0..1: the thread draws out, widens as it diffuses, then fades to
    // the residue level. `settled` draws the end state for the residue.
    function drawBloom(c, b, t, settled) {
      const grow = settled ? 1 : 1 - Math.pow(1 - Math.min(1, t / 0.7), 3);
      const fade = settled ? 0.16 : t < 0.5 ? 1 : 1 - ((t - 0.5) / 0.5) * 0.7;
      const spread = settled ? 3 : 1 + t * 2.2;
      c.save();
      c.globalCompositeOperation = "lighter";
      c.lineCap = "round";
      c.lineJoin = "round";
      for (const f of b.fil) {
        const k = Math.max(1, Math.floor(grow * (f.pts.length - 1)));
        c.beginPath();
        c.moveTo(f.pts[0][0], f.pts[0][1]);
        for (let i = 1; i <= k; i += 1) c.lineTo(f.pts[i][0], f.pts[i][1]);
        c.strokeStyle = `rgba(201, 164, 92, ${f.a * fade * 0.16})`;
        c.lineWidth = f.w * spread * 6 * dpr;
        c.stroke();
        c.strokeStyle = `rgba(201, 164, 92, ${f.a * fade})`;
        c.lineWidth = f.w * spread * dpr;
        c.stroke();
      }
      const cloud = c.createRadialGradient(b.g.x, b.g.y, b.g.r * 0.9, b.g.x, b.g.y, b.g.r * (1.1 + grow * 0.9));
      cloud.addColorStop(0, `rgba(201, 164, 92, ${0.07 * fade})`);
      cloud.addColorStop(1, "rgba(201, 164, 92, 0)");
      c.fillStyle = cloud;
      c.fillRect(0, 0, W, H);
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

    function bloom() {
      if (!W) return;
      const b = newBloom();
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
    function settle(n) {
      for (let i = 0; i < n; i += 1) bake(newBloom());
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

    let pending = 0;
    new ResizeObserver(() => {
      if (pending) return;
      pending = requestAnimationFrame(() => {
        pending = 0;
        resize();
      });
    }).observe($("#field"));

    return { bloom, settle, resize };
  })();

  // The watch face: ticks once, the room-left arc and the dial count on demand.
  (() => {
    const g = $("#ticks");
    let out = "";
    for (let i = 0; i < 12; i += 1) {
      const a = (i / 12) * Math.PI * 2 - Math.PI / 2;
      const major = i % 3 === 0;
      const r1 = major ? 98 : 100;
      const r2 = 106;
      const p = (r) => `${(160 + Math.cos(a) * r).toFixed(2)} ${(240 + Math.sin(a) * r).toFixed(2)}`;
      const [x1, y1] = p(r1).split(" ");
      const [x2, y2] = p(r2).split(" ");
      out += `<line${major ? ' class="major"' : ""} x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`;
    }
    g.innerHTML = out;
  })();

  function setRoomArc(st) {
    const arc = $("#room-arc");
    const pct =
      st && st.connected && st.capacity_bytes ? Math.max(0, Math.min(100, (st.free_bytes / st.capacity_bytes) * 100)) : 0;
    arc.style.strokeDasharray = `${pct.toFixed(2)} 100`;
    // The first value lands without motion; later changes of room ease in.
    if (!arc.classList.contains("live")) requestAnimationFrame(() => arc.classList.add("live"));
  }

  function setDial(count, word) {
    $("#dial-count").textContent = count == null ? "" : String(count);
    $("#dial-word").textContent = word || "";
  }

  // ------------------------------------------------------------ IPC

  const TAURI = window.__TAURI__;
  const DEMO = !TAURI;

  function tauriApi() {
    const invoke = (cmd, args) => TAURI.core.invoke(cmd, args);
    const onProgress = (cb) => TAURI.event.listen("pelican://progress", (e) => cb(e.payload));
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
    return { invoke, onProgress, onDrop };
  }

  const api = DEMO ? mockApi() : tauriApi();

  // Errors from the shell arrive as strings; a busy device gets a calm line.
  function errText(e) {
    const s = typeof e === "string" ? e : (e && (e.message || e.error)) || String(e);
    if (/\bbusy\b/i.test(s)) return "The watch is busy with another task. Try again when it finishes.";
    return s;
  }

  // ------------------------------------------------------------ state

  const S = {
    view: "watch",
    status: null,
    root: null,
    lib: null,
    rootEditing: false,
    chosen: [], // [{ path, name, kind: "dir" | "file", count? }]
    overrides: { artist: "", album: "", genre: "", year: "" },
    mix: { on: false, name: "" },
    order: null, // explicit per-file order once a mix is reordered
    resend: false,
    preview: null,
    previewing: false,
    previewError: null,
    previewSeq: 0,
    run: null,
    watchRows: null,
    watchError: null,
    ledger: null,
    ledgerError: null,
  };

  const STEP_OF = { watch: 0, choose: 1, library: 1, review: 2, send: 3, done: 3 };
  const FLOW = Object.keys(STEP_OF);
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

  // ------------------------------------------------------------ views

  const RENDER = {
    watch: renderWatch,
    choose: renderChoose,
    library: renderLibrary,
    review: renderReview,
    send: renderSend,
    done: renderDone,
    onwatch: renderOnWatch,
    ledger: renderLedger,
  };

  function show(view, { focus = true } = {}) {
    const prev = S.view;
    S.view = view;
    if (FLOW.includes(view)) flowView = view;
    document.body.dataset.view = view;
    for (const sec of $$(".view")) sec.hidden = sec.id !== `v-${view}`;
    const sec = $(`#v-${view}`);
    RENDER[view]();
    if (focus && prev !== view && !reduceMotion.matches) {
      sec.classList.remove("enter");
      void sec.offsetWidth;
      sec.classList.add("enter");
    }
    syncChrome();
    if (focus && prev !== view) {
      const h = $(".title", sec);
      if (h) h.focus({ preventScroll: true });
    }
  }

  function syncChrome() {
    const step = STEP_OF[S.view];
    const busy = running();
    const have = S.chosen.length > 0;
    $$("#steps button").forEach((b, i) => {
      const name = b.dataset.step;
      b.toggleAttribute("aria-current", i === step);
      if (i === step) b.setAttribute("aria-current", "step");
      const enabled =
        !busy &&
        i !== step &&
        (name === "watch" || name === "choose" || (name === "review" && have) || (name === "send" && S.run && S.run.finished));
      b.disabled = !enabled;
    });
    $$(".aside-nav .link").forEach((b) => {
      const cur = b.dataset.go === S.view;
      if (cur) b.setAttribute("aria-current", "page");
      else b.removeAttribute("aria-current");
      b.disabled = busy;
    });
    const connected = S.status && S.status.connected;
    document.body.classList.toggle("no-watch", !connected);
    setRoomArc(S.status);
    if (S.view === "send" || S.view === "done") {
      setDial(S.run ? S.run.tally.verified : 0, "verified");
    } else {
      setDial(null, "");
    }
  }

  // --- Watch -----------------------------------------------------------

  function classify(err) {
    const e = String(err || "").toLowerCase();
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
      const kind = classify(st.error);
      const title = {
        none: "Connect your watch",
        permission: "Permission needed",
        gvfs: "Release the watch",
        busy: "The watch is busy",
      }[kind];
      const line =
        st.error ||
        "Plug the watch in with a data cable. If it is plugged in, install udev/70-garmin-mtp.rules, or release it from the file manager with gio mount -u.";
      v.innerHTML = `<h1 class="title" id="t-watch" tabindex="-1">${esc(title)}</h1>
        <p class="lede">${withCode(line)}</p>
        <div class="actions">
          <button type="button" class="act" data-act="recheck">Check again</button>
          <button type="button" class="text-btn" data-go="choose">Choose music meanwhile</button>
        </div>`;
      return;
    }
    const room = roomFor(st);
    const facts = [
      `${plural(st.music_objects ?? 0, "track")} of ${st.max_objects ?? 500} on the watch`,
      `${(st.ledger?.verified ?? 0).toLocaleString("en")} sent by Pelican`,
    ];
    if (st.ledger?.failed) facts.push(`${plural(st.ledger.failed, "failed attempt")} in the ledger`);
    const warn = st.gvfs_warning
      ? `<p class="warn-line">${mark("warn")}<span><span class="word">Warning</span> ${withCode(st.gvfs_warning)}</span></p>`
      : "";
    v.innerHTML = `<h1 class="title" id="t-watch" tabindex="-1">${esc(st.model || "Your watch")}</h1>
      <p class="lede"><span class="gold">${esc(bytes(st.free_bytes))} free</span> · room for about ${room.toLocaleString("en")} tracks</p>
      <p class="facts">${esc(facts.join(" · "))}</p>
      ${warn}
      <div class="actions">
        <button type="button" class="act" data-go="choose">Choose music</button>
        ${S.chosen.length ? `<button type="button" class="text-btn" data-go="review">Review what is chosen</button>` : ""}
      </div>`;
  }

  async function refreshStatus() {
    try {
      S.status = await api.invoke("status");
    } catch (e) {
      S.status = { connected: false, error: errText(e) };
    }
    if (S.view === "watch") renderWatch();
    syncChrome();
  }

  // --- Choose ----------------------------------------------------------

  function renderChoose() {
    const v = $("#v-choose");
    const list = S.chosen.length
      ? `<ul class="chosen" aria-label="Chosen">${S.chosen
          .map(
            (c, i) => `<li>
              <span class="name">${esc(c.name)}<span class="where">${esc(dirname(c.path))}</span></span>
              <span class="dim small">${c.kind === "dir" ? (c.count != null ? plural(c.count, "track") : "folder") : "track"}</span>
              <button type="button" class="text-btn" data-act="unchoose" data-i="${i}" aria-label="Leave out ${esc(c.name)}">Leave out</button>
            </li>`,
          )
          .join("")}</ul>`
      : `<p class="empty-teach">Nothing chosen yet. Whole albums work best: each track is tagged from its file, or from its folder names when the file has none.</p>`;
    v.innerHTML = `<h1 class="title" id="t-choose" tabindex="-1">Choose music</h1>
      <p class="lede">Drop albums or folders anywhere on this window, or pick them from your library.</p>
      ${list}
      <div class="actions">
        ${S.chosen.length ? `<button type="button" class="act" data-go="review">Review</button>` : ""}
        <button type="button" class="${S.chosen.length ? "text-btn" : "act"}" data-go="library">Open the library</button>
      </div>
      <p class="quiet small">Library folder: <span class="path">${esc(S.root || "…")}</span></p>`;
  }

  function addChosen(entries) {
    let added = 0;
    for (const e of entries) {
      if (S.chosen.some((c) => c.path === e.path)) continue;
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

  // --- Library ---------------------------------------------------------

  async function loadLibrary(path) {
    S.lib = { loading: true, path };
    if (S.view === "library") renderLibrary();
    try {
      S.lib = await api.invoke("library_list", { path });
    } catch (e) {
      S.lib = { path, error: errText(e), dirs: [], files: [] };
    }
    if (S.view === "library") renderLibrary();
  }

  function crumbs(path) {
    const root = S.root || "/";
    const inside = path.startsWith(root) ? path.slice(root.length).split("/").filter(Boolean) : [];
    const parts = [{ name: basename(root) || root, path: root }];
    let acc = root;
    for (const seg of inside) {
      acc = `${acc.replace(/\/$/, "")}/${seg}`;
      parts.push({ name: seg, path: acc });
    }
    return parts
      .map((p, i) =>
        i === parts.length - 1
          ? `<span aria-current="location">${esc(p.name)}</span>`
          : `<button type="button" data-act="lib-open" data-path="${esc(p.path)}">${esc(p.name)}</button><span aria-hidden="true">/</span>`,
      )
      .join("");
  }

  function renderLibrary() {
    const v = $("#v-library");
    const lib = S.lib;
    const picked = new Set(S.chosen.map((c) => c.path));
    const form = S.rootEditing
      ? `<form class="root-form" data-form="root">
          <label class="sr-only" for="root-input">Library folder</label>
          <input class="field-input" id="root-input" name="root" value="${esc(S.root || "")}" spellcheck="false" autocomplete="off" placeholder="/path/to/music">
          <button type="submit" class="act">Use this folder</button>
          <button type="button" class="text-btn" data-act="root-cancel">Cancel</button>
        </form>`
      : "";
    let body;
    if (!lib || lib.loading) {
      body = `<ul class="rows" aria-busy="true">${skeletonRows(6)}</ul>`;
    } else if (lib.error) {
      body = `<div class="rows"><p class="empty"><strong>This folder cannot be read.</strong>${esc(lib.error)} Choose another library folder.</p></div>`;
    } else if (!lib.dirs.length && !lib.files.length) {
      body = `<div class="rows"><p class="empty"><strong>No music in this folder.</strong>Pelican reads MP3, FLAC, WAV, AAC/M4A, Ogg, Opus, WMA, APE and AIFF. Open a folder that holds albums, or change the library folder.</p></div>`;
    } else {
      const dirs = lib.dirs
        .map((d) => {
          const has = d.audio_files > 0 || d.has_subdirs;
          const sub =
            d.audio_files > 0
              ? `${plural(d.audio_files, "track")}${d.has_subdirs ? " · folders inside" : ""}`
              : d.has_subdirs
                ? "folders inside"
                : "no music";
          const id = `pick-${hashStr(d.path)}`;
          return `<li class="row lib-row">
            <input type="checkbox" class="check" id="${id}" data-act="pick" data-kind="dir" data-path="${esc(d.path)}" data-name="${esc(d.name)}" data-count="${d.audio_files}"${picked.has(d.path) ? " checked" : ""}${has ? "" : " disabled"}>
            <label for="${id}"><span class="t">${esc(d.name)}<span class="sub">${esc(sub)}</span></span></label>
            <span></span>
            ${has ? `<button type="button" class="open-btn" data-act="lib-open" data-path="${esc(d.path)}" aria-label="Open ${esc(d.name)}">Open${mark("open")}</button>` : "<span></span>"}
          </li>`;
        })
        .join("");
      const files = lib.files
        .map((f) => {
          const id = `pick-${hashStr(f.path)}`;
          return `<li class="row lib-row file">
            <input type="checkbox" class="check" id="${id}" data-act="pick" data-kind="file" data-path="${esc(f.path)}" data-name="${esc(f.name)}"${picked.has(f.path) ? " checked" : ""}>
            <label for="${id}"><span class="t">${esc(f.name)}</span></label>
            <span class="size">${esc(bytes(f.bytes))}</span>
          </li>`;
        })
        .join("");
      body = `<ul class="rows" aria-label="Folders and tracks">${dirs}${files}</ul>`;
    }
    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-library" tabindex="-1">Library</h1>
        <nav class="crumbs" aria-label="Folder">${crumbs((lib && lib.path) || S.root || "/")}
          <span aria-hidden="true">·</span><button type="button" data-act="root-edit">Change library folder</button></nav>
        ${form}
      </div>
      ${body}
      <div class="bar">
        <span class="count">${S.chosen.length ? `${plural(S.chosen.length, "item")} chosen` : "Tick albums, folders or tracks to choose them."}</span>
        <button type="button" class="text-btn" data-go="choose">Back</button>
        <button type="button" class="act" data-go="review"${S.chosen.length ? "" : " disabled"}>Review</button>
      </div>`;
    if (S.rootEditing) $("#root-input").focus();
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
      return `<span class="state s-send">${mark("pending")}Send</span><span class="size">est. ${esc(bytes(f.est_bytes))}</span>`;
    }
    if (f.verdict === "skip") {
      return `<span class="state s-skip">${mark("skipped")}Skip</span><span class="why dim">${esc(f.reason || "")}</span>`;
    }
    return `<span class="state s-refused">${mark("failed")}Refused</span><span class="why fail">${esc(f.reason || "")}</span>`;
  }

  function renderReview({ keepFocus = false } = {}) {
    const v = $("#v-review");
    if (!S.chosen.length) {
      v.innerHTML = `<div class="head"><h1 class="title" id="t-review" tabindex="-1">Review</h1></div>
        <div class="rows"><p class="empty"><strong>Nothing to review yet.</strong>Choose albums or tracks first; this is where you see how each will be tagged, whether it fits, and what gets sent.</p></div>
        <div class="bar"><button type="button" class="act" data-go="choose">Choose music</button></div>`;
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
      list = `<div class="rows"><p class="empty"><strong>Pelican could not read what was chosen.</strong>${esc(S.previewError)} Check the folders are still there (a NAS may need remounting), then try again.</p>
        <p class="empty"><button type="button" class="act" data-act="preview">Try again</button></p></div>`;
    } else if (p) {
      const t = p.totals;
      const bits = [`${plural(p.files.length, "track")}`];
      bits.push(`${t.send.toLocaleString("en")} to send`);
      if (t.skip) bits.push(`${t.skip.toLocaleString("en")} skipped`);
      if (t.refused) bits.push(`${t.refused.toLocaleString("en")} refused`);
      summary = `${bits.join(" · ")} · about ${bytes(t.est_bytes)}`;
      list = `<ol class="rows${S.previewing ? " is-stale" : ""}" aria-label="Tracks"${S.previewing ? ' aria-busy="true"' : ""}>${p.files
        .map((f, i) => {
          const sub = [f.artist, f.album, f.year, f.genre].filter(Boolean).join(" · ");
          const order = mixing
            ? `<span class="order">
                <button type="button" data-act="move" data-i="${i}" data-d="-1" aria-label="Move ${esc(f.title)} up"${i === 0 ? " disabled" : ""}><svg class="mk" aria-hidden="true"><use href="#m-up"/></svg></button>
                <button type="button" data-act="move" data-i="${i}" data-d="1" aria-label="Move ${esc(f.title)} down"${i === p.files.length - 1 ? " disabled" : ""}><svg class="mk" aria-hidden="true"><use href="#m-down"/></svg></button>
              </span>`
            : "";
          return `<li class="row track-row is-${esc(f.verdict)}">
            <span class="num">${esc(f.track ?? "–")}</span>
            <span class="t">${esc(f.title)}<span class="sub">${esc(sub || "No artist or album")}</span></span>
            <span class="end">${verdictCell(f)}</span>
            ${order}
          </li>`;
        })
        .join("")}</ol>
        <p class="est-note">Sizes are estimates at the one profile (192 kbps MP3); the exact size is known once each track is converted.</p>`;
    }

    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-review" tabindex="-1">Review</h1>
        <p class="quiet">${esc(summary)}</p>
      </div>
      <div class="review-body${mixing ? " mixing" : ""}">
        <div class="review-list">${list}</div>
        <div class="review-side">
          ${fitBlock(p)}
          <fieldset>
            <legend class="group-label">For every track</legend>
            <div class="overrides">
              ${overrideInput("artist", "Artist")}
              ${mixing ? "" : overrideInput("album", "Album")}
              ${overrideInput("genre", "Genre")}
              ${overrideInput("year", "Year", 'inputmode="numeric" maxlength="4"')}
            </div>
          </fieldset>
          <div>
            <label class="toggle"><input type="checkbox" class="check" id="mix-on" data-act="mix-toggle"${mixing ? " checked" : ""}>
              <span>Send as a mix<span class="dim">It appears on the watch as an album, in this order.</span></span></label>
            ${
              mixing
                ? `<label class="sr-only" for="mix-name">Mix name</label>
                   <input class="field-input mix-name" id="mix-name" data-input="mix" value="${esc(S.mix.name)}" placeholder="Name the mix, e.g. Long Run" autocomplete="off">`
                : ""
            }
          </div>
          ${
            (p && p.totals.skip) || S.resend
              ? `<label class="toggle"><input type="checkbox" class="check" id="resend" data-act="resend"${S.resend ? " checked" : ""}>
                  <span>Send skipped tracks again<span class="dim">Each becomes a second entry on the watch.</span></span></label>`
              : ""
          }
          <div class="commit">
            <p class="permanence">What goes on the watch stays on it — tracks remain until the watch is reset.</p>
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
        <span class="dim">About <span class="gold">${esc(bytes(Math.max(0, left)))}</span> will remain · ${after.toLocaleString("en")} of ${st.max_objects ?? 500} tracks after</span></div>`;
    }
    return `<div class="fit s-failed">${mark("failed")}<span class="word">Does not fit</span>
      <span class="dim">${esc(f.reason || "")} Leave some tracks out and review again.</span></div>`;
  }

  function sendBlock(p) {
    let why = "";
    const n = p ? p.totals.send : 0;
    if (!S.status || !S.status.connected) why = "Connect the watch to send.";
    else if (!p) why = "";
    else if (S.mix.on && !S.mix.name.trim()) why = "Name the mix to send it.";
    else if (!p.fits.ok) why = "It does not fit on the watch as chosen.";
    else if (!n) why = "Nothing here needs sending.";
    const ok = p && !why && !S.previewing;
    return `<div class="send-block">
      <button type="button" class="act send" data-act="send"${ok ? "" : " disabled"}>${n ? `Send ${plural(n, "track")}` : "Send"}</button>
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
          ? `again as ${ev.remote} — the first copy did not read back the same`
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
          announce(`Verified: ${line.title}`);
        } else if (ev.outcome === "skipped") {
          line.detail = ev.reason || "";
          run.tally.skipped += 1;
        } else {
          line.detail = ev.reason || "";
          run.tally.failed += 1;
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
        announce(`Send stopped: ${ev.message}`);
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
    if (ev.kind === "error") renderSend();
    if (ev.kind === "done") setDial(run.tally.verified, "verified");
  }

  function lineHtml(l) {
    const meter =
      l.state === "sending" ? `<span class="meter" aria-hidden="true"><i data-w="${l.pct}"></i></span>` : "";
    return `<span class="ct">${esc(l.title)}</span>
      <span class="cs">${mark(LINE_MARK[l.state])}<span class="word">${LINE_WORD[l.state]}</span>${
        l.detail ? `<span class="detail">${esc(l.detail)}</span>` : ""
      }${meter}</span>`;
  }

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
    li.className = `credit is-${l.state}${fresh && !reduceMotion.matches ? " rise" : ""}`;
    li.innerHTML = lineHtml(l);
    const bar = li.querySelector(".meter i");
    if (bar) bar.style.transform = `scaleX(${l.pct.toFixed(3)})`;
    if (resolved && !reduceMotion.matches) li.querySelector(".cs").classList.add("resolve");
    follow(li);
  }

  function follow(li) {
    const roll = $("#roll");
    if (!roll || Date.now() - rollTouchedAt < 5000) return;
    const top = li.offsetTop - roll.clientHeight * 0.58 + li.offsetHeight / 2;
    roll.scrollTo({ top, behavior: reduceMotion.matches ? "auto" : "smooth" });
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
    m.innerHTML = `${esc(bits.join(" · "))}${run.name ? ` · <span class="dim">as the mix “${esc(run.name)}”</span>` : ""}`;
    const stop = $("#stop");
    if (stop) {
      stop.disabled = run.stopping || !!run.finished || !!run.error;
      stop.textContent = run.stopping ? "Stopping after this track" : "Stop after this track";
    }
  }

  function renderSend() {
    const v = $("#v-send");
    const run = S.run;
    if (!run) {
      v.innerHTML = `<h1 class="title" id="t-send" tabindex="-1">Nothing is sending</h1>
        <p class="lede dim">Choose music and review it first; Send starts from Review.</p>
        <div class="actions"><button type="button" class="act" data-go="choose">Choose music</button></div>`;
      return;
    }
    if (run.error) {
      v.innerHTML = `<h1 class="title" id="t-send" tabindex="-1">Send interrupted</h1>
        <p class="lede"><span class="state s-failed">${mark("failed")}<span class="word">Failed</span></span> ${withCode(run.error)}</p>
        <p class="quiet">Tracks that finished before this are on the watch and in the ledger. Check the cable and the watch, then review again: what is already there will be skipped.</p>
        <div class="actions"><button type="button" class="act" data-act="back-review">Back to review</button>
        <button type="button" class="text-btn" data-go="ledger">Open the ledger</button></div>`;
      return;
    }
    v.innerHTML = `<h1 class="title" id="t-send" tabindex="-1">Sending</h1>
      <p class="roll-meta" id="roll-meta"></p>
      <div class="roll" id="roll" tabindex="0" aria-label="Tracks as they are sent">
        <ol class="roll-list" id="roll-list" role="log" aria-live="off"></ol>
      </div>
      <div class="stop-line">
        <button type="button" class="act" id="stop" data-act="stop">Stop after this track</button>
        <span>A track in flight is always finished and proven first.</span>
      </div>`;
    const roll = $("#roll-list");
    roll.innerHTML = run.lines
      .map((l, i) =>
        l.shown || l.state !== "pending" ? `<li class="credit is-${l.state}" data-i="${i}">${lineHtml(l)}</li>` : "",
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
    paintMeta();
    const last = roll.lastElementChild;
    if (last) {
      requestAnimationFrame(() => {
        r.scrollTop = last.offsetTop - r.clientHeight * 0.58 + last.offsetHeight / 2;
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
        "Every verified track was read back from the watch and matched by hash. Unplug the watch; the music appears in its library once it reconnects.",
      );
    }
    if (f.stopped) words.push("Stopped between tracks: the ones not reached were not sent and took no name.");
    if (fails.length) words.push("A failed track can be sent again from Review; it goes under a new name.");
    v.innerHTML = `<h1 class="title" id="t-done" tabindex="-1">${esc(title)}</h1>
      <p class="tally">${tally}</p>
      ${words.map((w) => `<p class="quiet">${esc(w)}</p>`).join("")}
      ${
        fails.length
          ? `<ul class="failures" aria-label="Failed tracks">${fails
              .map((l) => `<li>${esc(l.title)}<span class="fail">${mark("failed")} Failed · ${esc(l.detail)}</span></li>`)
              .join("")}</ul>`
          : ""
      }
      <details class="every"><summary>Every track in this send</summary><ol>${run.lines
        .map(
          (l) =>
            `<li><span class="t">${esc(l.title)}</span><span class="state s-${l.state}">${mark(LINE_MARK[l.state])}${LINE_WORD[l.state]}${
              l.state === "verified" && l.sha ? ` <span class="dim">${esc(l.sha.slice(0, 8))}</span>` : ""
            }</span></li>`,
        )
        .join("")}</ol></details>
      <div class="actions">
        <button type="button" class="act" data-act="again">Choose more music</button>
        <button type="button" class="text-btn" data-go="onwatch">See what is on the watch</button>
      </div>`;
    setDial(f.verified, "verified");
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
      body = `<div class="rows"><p class="empty"><strong>${esc(S.watchError)}</strong>Connect the watch, close anything else that has it open, and check again.</p>
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
            <span class="state s-${r.status}">${mark(WATCH_MARK[r.status])}${WATCH_WORD[r.status]}</span>
            <span class="t">${esc(t)}${sub ? `<span class="sub">${esc(sub)}</span>` : ""}</span>
            <span class="size">${r.bytes != null ? esc(bytes(r.bytes)) : ""}</span>
          </li>`;
        })
        .join("")}</ul>`;
    }
    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-onwatch" tabindex="-1">On the watch</h1>
        <p class="quiet">${esc(summary)}</p>
        <p class="honest">Pelican cannot remove anything from the watch: what is on it stays until the watch is reset.</p>
      </div>
      ${body}
      <div class="bar"><span class="count">Other: put there by another app or computer.</span>
        <button type="button" class="text-btn" data-act="back-flow">Back</button></div>`;
  }

  // --- Ledger ----------------------------------------------------------

  const EVENT_WORD = { reserve: "Reserved", verified: "Verified", failed: "Failed" };
  const EVENT_MARK = { reserve: "pending", verified: "verified", failed: "failed" };

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
    return d.toLocaleString("en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  }

  function renderLedger() {
    const v = $("#v-ledger");
    const lg = S.ledger;
    let where = "";
    let body = `<ul class="rows" aria-busy="true">${skeletonRows(7)}</ul>`;
    if (S.ledgerError) {
      body = `<div class="rows"><p class="empty"><strong>The ledger could not be read.</strong>${esc(S.ledgerError)}</p></div>`;
    } else if (lg) {
      where = `<p class="quiet small"><span class="path">${esc(lg.path)}</span> · append-only; a name written here is never used again.</p>`;
      body = lg.rows.length
        ? `<ul class="rows" aria-label="Ledger, newest first">${lg.rows
            .slice()
            .reverse()
            .map((r) => {
              const who = [r.title, r.artist, r.album].filter(Boolean).join(" · ");
              return `<li class="row ledger-row">
                <span class="num">${esc(String(r.counter).padStart(5, "0"))}</span>
                <span class="state s-${r.event}">${mark(EVENT_MARK[r.event] || "other")}${EVENT_WORD[r.event] || esc(r.event)}</span>
                <span class="t">${esc(r.remote)}<span class="sub">${esc(who)}</span>${r.reason ? `<span class="why fail">${esc(r.reason)}</span>` : ""}</span>
                <span class="when">${esc(when(r.at))}</span>
              </li>`;
            })
            .join("")}</ul>`
        : `<div class="rows"><p class="empty"><strong>No sends from this computer yet.</strong>Before each upload Pelican writes the track's new name here, so no name is ever used twice on this watch.</p></div>`;
    }
    v.innerHTML = `<div class="head">
        <h1 class="title" id="t-ledger" tabindex="-1">Ledger</h1>
        ${where}
      </div>
      ${body}
      <div class="bar"><span class="count">${lg ? `${plural(lg.rows.length, "line")}` : ""}</span>
        <button type="button" class="text-btn" data-act="back-flow">Back</button></div>`;
  }

  // ------------------------------------------------------------ events

  function go(view) {
    if (running() && view !== "send") return;
    clearNotice();
    if (view === "library" && (!S.lib || S.lib.error)) loadLibrary(S.root);
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
    const el = e.target.closest("[data-act]");
    if (!el || el.disabled) return;
    const act = el.dataset.act;
    switch (act) {
      case "dismiss":
        clearNotice();
        break;
      case "recheck":
        S.status = null;
        renderWatch();
        await refreshStatus();
        break;
      case "unchoose":
        S.chosen.splice(Number(el.dataset.i), 1);
        invalidatePreview();
        renderChoose();
        syncChrome();
        break;
      case "lib-open":
        loadLibrary(el.dataset.path);
        break;
      case "root-edit":
        S.rootEditing = true;
        renderLibrary();
        break;
      case "root-cancel":
        S.rootEditing = false;
        renderLibrary();
        break;
      case "preview":
        runPreview();
        renderReview();
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
        go("choose");
        break;
      case "watch-reload":
        loadWatchRows();
        break;
      case "back-flow":
        go(flowView);
        break;
      default:
        break;
    }
  });

  document.addEventListener("change", (e) => {
    const el = e.target;
    const act = el.dataset.act;
    if (act === "pick") {
      const path = el.dataset.path;
      if (el.checked) {
        addChosen([
          {
            path,
            name: el.dataset.name,
            kind: el.dataset.kind,
            count: el.dataset.kind === "dir" ? Number(el.dataset.count) : undefined,
          },
        ]);
      } else {
        S.chosen = S.chosen.filter((c) => c.path !== path);
        invalidatePreview();
      }
      const count = $("#v-library .bar .count");
      if (count) {
        count.textContent = S.chosen.length
          ? `${plural(S.chosen.length, "item")} chosen`
          : "Tick albums, folders or tracks to choose them.";
      }
      const rev = $('#v-library .bar [data-go="review"]');
      if (rev) rev.disabled = !S.chosen.length;
      syncChrome();
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
    const form = e.target.closest("[data-form='root']");
    if (!form) return;
    e.preventDefault();
    const path = form.root.value.trim();
    if (!path) return;
    try {
      S.root = await api.invoke("set_library_root", { path });
      S.rootEditing = false;
      loadLibrary(S.root);
    } catch (err) {
      notice(errText(err), { error: true });
    }
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && S.rootEditing) {
      S.rootEditing = false;
      renderLibrary();
    }
  });

  window.addEventListener("focus", () => {
    if (S.view === "watch" && S.status && !S.status.connected && !running()) refreshStatus();
  });

  // Drops: the shell hands absolute paths; a browser (demo) cannot.
  function onDrop(kind, paths) {
    if (running()) return;
    if (kind === "over") document.body.classList.add("dragging");
    else if (kind === "leave") document.body.classList.remove("dragging");
    else if (kind === "drop") {
      document.body.classList.remove("dragging");
      const added = addChosen(
        paths.map((p) => ({ path: p, name: basename(p), kind: /\.[a-z0-9]{2,4}$/i.test(p) ? "file" : "dir" })),
      );
      if (S.view !== "choose") go("choose");
      else renderChoose();
      syncChrome();
      if (!added && paths.length) notice("Already chosen.", { word: "Note" });
    }
  }

  if (api.onDrop) api.onDrop(onDrop);
  api.onProgress(onProgress);

  // ------------------------------------------------------------ boot

  async function boot() {
    if (DEMO) {
      $("#demo-flag").hidden = false;
      await demoState(location.hash.slice(1) || "watch");
      window.addEventListener("hashchange", () => demoState(location.hash.slice(1) || "watch"));
      return;
    }
    show("watch", { focus: false });
    try {
      S.root = await api.invoke("library_root");
    } catch (_) {
      S.root = null;
    }
    await refreshStatus();
  }

  // ================================================================ demo
  //
  // The IPC contract, served in-file from the owner's real library layout
  // (names, sizes and durations read off the NAS), so the window can be
  // seen and screenshotted with no watch attached. Everything below is
  // demonstration data and is labelled so in the window.

  function mockApi() {
    const ROOT = "/mnt/nas/Music";
    const SOT_DIR = `${ROOT}/Sea of Thieves`;
    const MC_ARTIST = `${ROOT}/Iva Davies, Christopher Gordon, Richard Tognetti`;
    const MC_DIR = `${MC_ARTIST}/Master and Commander The Far Side of the World (Music from the Motion Picture)`;
    const WR_DIR = `${ROOT}/Windrose`;

    // Sea of Thieves: 25 untagged WAVs; tags come from the path.
    const SOT = [
      ["We Shall Sail Together", 36421736, 126.5],
      ["Maiden Voyage", 45909490, 158.6],
      ["Blessing of Athena's Fortune", 65849218, 227.4],
      ["A New Dawn", 66816104, 232.0],
      ["Shroudbroken", 48582328, 168.7],
      ["Ballad Of The Mer", 26863262, 93.3],
      ["Treasury Ambush", 54432104, 189.0],
      ["A Star To Sail By", 33159294, 186.5],
      ["Herald Of The Flame", 28966992, 100.0],
      ["Gold Hoarder", 59692816, 207.3],
      ["Descend Into The Reaper's Lair", 16498194, 57.0],
      ["Sunken Depths", 59643762, 206.0],
      ["Spectral Sails", 112194710, 389.6],
      ["Who Shall Not Be Returning", 72093274, 249.0],
      ["With Hammer And Hope", 50112104, 174.0],
      ["Fated Enemies", 40709238, 140.6],
      ["The Risen", 59964530, 207.1],
      ["Ritual of the Flame", 68526188, 237.9],
      ["Sirens' Lament", 34622294, 120.2],
      ["Shores Of Plenty", 58782304, 204.1],
      ["Strongholds Of The Sea", 92078140, 318.0],
      ["The Shrouded Ghost", 74155588, 257.5],
      ["Captains Of Adventure - Sunrise", 116388868, 402.0],
      ["The Wild Rose", 62004602, 215.3],
      ["Becalmed - Shores Of Gold", 56024722, 193.5],
    ];

    // Master and Commander: tagged FLAC. [track, file stem, bytes, seconds, title, artist]
    const MC = [
      [1, "Endless Ocean - Tognetti The Far Side of the World", 46255779, 559.1, "Endless Ocean Tognetti: The Far Side of the World", "Iva Davies"],
      [2, "Ghost of Time - Tognetti Into the Fog", 11633252, 132.3, "Ghost of Time Tognetti: Into the Fog", "Iva Davies"],
      [3, "Violin Concerto No.3 in G, K.216 - III. Andante-Allegretto (Excerpt Arr. Tognetti)", 6394811, 78.7, "Violin Concerto No.3 in G, K.216 III. Andante-Allegretto (Excerpt Arr. Tognetti)", "Richard Tognetti"],
      [4, "The Cuckold Comes out of the Amery - The Cuckold Comes out of the Amery (Arr. Davies, Gordon and Tognetti)", 22067265, 206.2, "The Cuckold Comes out of the Amery (Arr. Davies, Gordon and Tognetti)", "Richard Tognetti"],
      [5, "Smoke n'oakum (Master & Commander - OST) - Tognetti Smoke n'oakum", 21982350, 326.4, "Smoke n'oakum (Master & Commander - OST) Tognetti: Smoke n'oakum", "Iva Davies"],
      [6, "Fantasia On A Theme By Thomas Tallis - Vaughan Williams Fantasia on a Theme by Thomas Tallis (Excerpt)", 27229891, 311.1, "Fantasia On A Theme By Thomas Tallis Vaughan Williams: Fantasia on a Theme by Thomas Tallis (Excerpt)", "New Queen's Hall Orchestra"],
      [8, "The Doldrums (Master & Commander - OST) - Tognetti The Doldrums", 12515400, 165.1, "The Doldrums (Master & Commander - OST) Tognetti: The Doldrums", "Iva Davies"],
      [9, "Suite pour violoncelle seul n° 1 en sol majeur, BWV 1007 - 1. Prélude", 12864520, 148.0, "Suite pour violoncelle seul n° 1 en sol majeur, BWV 1007 1. Prélude", "Yo-Yo Ma"],
      [10, "The Galapagos (Master & Commander - OST) - Tognetti The Galapagos", 8355072, 98.2, "The Galapagos (Master & Commander - OST) Tognetti: The Galapagos", "Iva Davies"],
      [11, "Nancy Dawson - Folk Medley (Arr. Davies, Gordon and Tognetti)", 30272492, 310.2, "Nancy Dawson Folk Medley (Arr. Davies, Gordon and Tognetti)", "Richard Tognetti"],
      [12, "The Phasmid (Master & Commander - OST) - Tognetti The Phasmid", 13735957, 154.9, "The Phasmid (Master & Commander - OST) Tognetti: The Phasmid", "Iva Davies"],
      [13, "The Battle (Master & Commander - OST) - Tognetti The Battle", 35078934, 305.6, "The Battle (Master & Commander - OST) Tognetti: The Battle", "Iva Davies"],
      [15, "Full Circle (Master & Commander - OST) - Tognetti Full Circle", 5848065, 94.5, "Full Circle (Master & Commander - OST) Tognetti: Full Circle", "Iva Davies"],
    ];
    const MC_ALBUM = "Master and Commander: The Far Side of the World (Music from the Motion Picture)";

    // Windrose: 33 untagged WAVs.
    const WR = [
      ["01_Drunken Sailor (Trailer Version).wav", 11691468], ["02_Rolling Down to Old Maui (Trailer Version).wav", 13118140],
      ["03_Rolling Home (Trailer Version).wav", 28777708], ["04_The British Tars (Trailer Version).wav", 15357036],
      ["05_Drunken Sailor.WAV", 14398084], ["06_Rolling Down to Old Maui.WAV", 15943916], ["07_Rolling Home.WAV", 20135352],
      ["08_The British Tars.WAV", 14862560], ["09_Leave her Johnny.WAV", 22125744], ["10_Blow the Man Down.WAV", 19315736],
      ["11_Bully in the Alley.WAV", 25331020], ["12_Good Morning Ladies.WAV", 17566112], ["13_Maggie May.WAV", 28742220],
      ["14_Whiskey Johnny.WAV", 10581440], ["15_Before the Breeze.WAV", 26962712], ["16_Blackbeard's Crew.WAV", 30542556],
      ["17_Local Threat.WAV", 35599992], ["18_Cannonade Chorus.WAV", 24643112], ["19_Steel on Timber.WAV", 44001704],
      ["20_Discovery.WAV", 55213936], ["21_Journey's Beginning.WAV", 42860668], ["22_Duel of Sails.WAV", 32209644],
      ["23_Holding Course.WAV", 26989936], ["24_Edge of the Abyss.WAV", 43571536], ["25_Freshwater Prelude.WAV", 38244772],
      ["26_Lunar Adagio.WAV", 39690736], ["27_Septachord Veil.WAV", 27147096], ["28_Starlight on a Swell.WAV", 40304376],
      ["29_Sunstone Morning.WAV", 36874976], ["30_Toccata of the Tempest.WAV", 47916448], ["31_The Hearth.WAV", 53795976],
      ["32_Tortuga.WAV", 37332964], ["33_Windward Bound.WAV", 52946476],
    ];

    const pad = (n) => String(n).padStart(2, "0");
    const FILES = new Map();
    SOT.forEach(([title, size, secs], i) => {
      const source = `${SOT_DIR}/${pad(i + 1)} - ${title}.wav`;
      FILES.set(source, { source, size, secs, tags: { title, artist: "Sea of Thieves", album: "Sea of Thieves", track: String(i + 1) } });
    });
    MC.forEach(([n, stem, size, secs, title, artist]) => {
      const source = `${MC_DIR}/${pad(n)} - Iva Davies, Christopher Gordon, Richard Tognetti - ${stem}.flac`;
      FILES.set(source, {
        source,
        size,
        secs,
        tags: { title, artist, album: MC_ALBUM, track: String(n), year: "2003", genre: "Film Soundtracks" },
      });
    });
    WR.forEach(([name, size]) => {
      const m = name.match(/^(\d+)_(.*)\.(wav)$/i);
      const source = `${WR_DIR}/${name}`;
      FILES.set(source, {
        source,
        size,
        secs: size / 176400,
        tags: { title: m[2], artist: "Windrose", album: "Windrose", track: String(Number(m[1])) },
      });
    });

    const slug = (t) => t.replace(/[\\/:*?"<>|]/g, "").slice(0, 48).trim();
    const fakeSha = (s) => {
      let out = "";
      let h = 0x811c9dc5;
      for (let r = 0; out.length < 64; r += 1) {
        for (let i = 0; i < s.length; i += 1) {
          h ^= s.charCodeAt(i) + r;
          h = Math.imul(h, 16777619);
        }
        out += (h >>> 0).toString(16).padStart(8, "0");
      }
      return out.slice(0, 64);
    };

    // The watch: Windrose 09–32 sent by Pelican (counters 2–25), the
    // hand-run libmtp test file, one other app's file, and two stubs left
    // by playlist writes the firmware rejected.
    const onWatch = [];
    const ledgerRows = [];
    const verifiedKeys = new Set();
    let counter = 1;
    const base = Date.parse("2026-09-20T18:12:00Z");
    WR.slice(8, 32).forEach(([name], k) => {
      counter += 1;
      const f = FILES.get(`${WR_DIR}/${name}`);
      const remote = `pl${String(counter).padStart(5, "0")}-${slug(f.tags.title)}.mp3`;
      const at = new Date(base + k * 2100).toISOString();
      const est = Math.ceil(f.secs * 24000) + 4096;
      onWatch.push({ status: "ledger", name: remote, bytes: est, title: f.tags.title, artist: "Windrose", album: "Windrose" });
      ledgerRows.push({ counter, remote, event: "reserve", at, title: f.tags.title, artist: "Windrose", album: "Windrose" });
      ledgerRows.push({ counter, remote, event: "verified", at: new Date(Date.parse(at) + 1900).toISOString(), title: f.tags.title, artist: "Windrose", album: "Windrose" });
      verifiedKeys.add(`${f.source}|Windrose`);
    });
    // Same shape as the shell's watch_list: tags only on ledger rows, and a
    // stub carries the core's synthetic name.
    onWatch.unshift({ status: "foreign", name: "pl0001.mp3", bytes: 3810304 });
    onWatch.push({ status: "foreign", name: "Morning Intervals.mp3", bytes: 5244012 });
    onWatch.push({ status: "stub", name: "\u2039unreadable #6021\u203a" }, { status: "stub", name: "\u2039unreadable #6022\u203a" });

    const status = {
      connected: true,
      model: "Forerunner 165 Music",
      serial: "3456789012",
      free_bytes: 2300000000,
      capacity_bytes: 3500000000,
      music_objects: onWatch.length,
      max_objects: 500,
      ledger: { verified: 24, failed: 0, unresolved: 0 },
    };

    let root = ROOT;
    let connected = true;
    const listeners = [];
    let stopFlag = false;
    let runSeq = 0;

    function expand(paths) {
      const out = [];
      for (const p of paths) {
        if (FILES.has(p)) out.push(FILES.get(p));
        else for (const [src, f] of FILES) if (src.startsWith(`${p.replace(/\/$/, "")}/`)) out.push(f);
      }
      return out;
    }

    function preview(req) {
      const files = expand(req.paths);
      const ov = req.overrides || {};
      const seen = new Map();
      let pos = 0;
      const rows = files.map((f) => {
        const t = { ...f.tags };
        if (ov.artist) t.artist = ov.artist;
        if (ov.album) t.album = ov.album;
        if (ov.genre) t.genre = ov.genre;
        if (ov.year) t.year = ov.year;
        if (req.mix) {
          pos += 1;
          t.album = req.mix.name;
          t.track = String(pos);
          t.year = ov.year;
          t.genre = ov.genre;
        }
        const est = Math.ceil(f.secs * 24000) + 4096;
        let verdict = "send";
        let reason;
        const key = `${f.source}|${t.album}`;
        if (seen.has(f.source)) {
          verdict = "skip";
          reason = `same audio as ${seen.get(f.source)}`;
        } else if (!req.resend && verifiedKeys.has(key)) {
          verdict = "skip";
          const row = onWatch.find((r) => r.status === "ledger" && r.title === t.title);
          reason = `already on watch as ${row ? row.name : "an earlier send"}`;
        }
        seen.set(f.source, f.source);
        return {
          source: f.source,
          title: t.title,
          artist: t.artist,
          album: t.album,
          track: t.track,
          year: t.year,
          genre: t.genre,
          source_bytes: f.size,
          est_bytes: est,
          verdict,
          reason,
        };
      });
      const totals = { send: 0, skip: 0, refused: 0, est_bytes: 0 };
      for (const r of rows) {
        totals[r.verdict] += 1;
        if (r.verdict === "send") totals.est_bytes += r.est_bytes;
      }
      const objectsAfter = status.music_objects + totals.send;
      const room = status.free_bytes - RESERVE_BYTES;
      let fits = { ok: true, free_bytes: status.free_bytes, objects_after: objectsAfter };
      if (totals.est_bytes > room) {
        fits = { ok: false, free_bytes: status.free_bytes, objects_after: objectsAfter, reason: `Needs about ${bytes(totals.est_bytes)}; ${bytes(status.free_bytes)} is free.` };
      } else if (objectsAfter > 500) {
        fits = { ok: false, free_bytes: status.free_bytes, objects_after: objectsAfter, reason: `That would make ${objectsAfter} tracks; the watch holds 500.` };
      }
      return { files: rows, totals, fits };
    }

    // What a finished file leaves behind: ledger lines, and on success an
    // entry on the watch that the next preview skips.
    function record(r, remote, n, outcome, reason) {
      const now = new Date().toISOString();
      const who = { title: r.title, artist: r.artist, album: r.album };
      ledgerRows.push({ counter: n, remote, event: "reserve", at: now, ...who });
      ledgerRows.push({ counter: n, remote, event: outcome === "verified" ? "verified" : "failed", at: now, reason, ...who });
      if (outcome !== "verified") {
        status.ledger.failed += 1;
        return;
      }
      onWatch.push({ status: "ledger", name: remote, bytes: r.est_bytes, ...who });
      verifiedKeys.add(`${r.source}|${r.album}`);
      status.music_objects += 1;
      status.free_bytes -= r.est_bytes;
      status.ledger.verified += 1;
    }

    function emit(ev) {
      for (const cb of listeners) cb(ev);
    }

    const wait = (ms) => new Promise((r) => setTimeout(r, ms));

    // Real pacing: about two seconds a track, as on the FR165.
    async function drive(id, rows, from, next, so_far) {
      stopFlag = false;
      let counterNow = next;
      const tally = { verified: 0, skipped: 0, failed: 0, ...so_far };
      const total = rows.length;
      for (let i = from; i < total; i += 1) {
        const r = rows[i];
        if (stopFlag) {
          emit({ run_id: id, kind: "done", index: i, source: r.source, outcome: "skipped", reason: "stopped before it was sent" });
          tally.skipped += 1;
          continue;
        }
        if (r.verdict === "skip") {
          await wait(260);
          emit({ run_id: id, kind: "done", index: i, source: r.source, outcome: "skipped", reason: r.reason });
          tally.skipped += 1;
          continue;
        }
        emit({ run_id: id, kind: "transcoding", index: i, total, source: r.source });
        await wait(620);
        const attempts = r.title === "Spectral Sails" || r.title === "Sunken Depths" ? 2 : 1;
        let outcome = "verified";
        let reason;
        let remote;
        for (let a = 0; a < attempts; a += 1) {
          counterNow += 1;
          remote = `pl${String(counterNow).padStart(5, "0")}-${slug(r.title)}.mp3`;
          emit({ run_id: id, kind: "sending", index: i, total, source: r.source, remote });
          for (let k = 1; k <= 5; k += 1) {
            await wait(210);
            emit({ run_id: id, kind: "uploading", index: i, bytes: Math.round((r.est_bytes * k) / 5), total_bytes: r.est_bytes });
          }
          await wait(160);
        }
        if (r.title === "Sunken Depths") {
          outcome = "failed";
          reason = "read-back mismatch: sent 4,948,096 bytes, read back 4,947,968; retry failed: the watch stopped answering (USB timeout)";
        }
        record(r, remote, counterNow, outcome, reason);
        emit({
          run_id: id,
          kind: "done",
          index: i,
          source: r.source,
          outcome,
          remote: outcome === "verified" ? remote : undefined,
          sha256: outcome === "verified" ? fakeSha(remote) : undefined,
          reason,
        });
        tally[outcome] += 1;
      }
      emit({ run_id: id, kind: "finished", ...tally, stopped: stopFlag });
    }

    async function invoke(cmd, args = {}) {
      await wait(cmd === "preview" ? 380 : 140);
      switch (cmd) {
        case "status":
          return connected
            ? JSON.parse(JSON.stringify(status))
            : {
                connected: false,
                max_objects: 500,
                ledger: status.ledger,
                error:
                  "No watch found. Use a data cable (a charge-only cable carries nothing); if the watch is plugged in, install udev/70-garmin-mtp.rules once, and if the file manager opened it, release it with gio mount -u mtp://Garmin/",
              };
        case "library_root":
          return root;
        case "set_library_root":
          if (!args.path.startsWith("/")) throw new Error("Give an absolute path, such as /mnt/nas/Music.");
          root = args.path;
          return root;
        case "library_list": {
          const path = args.path.replace(/\/$/, "");
          const dirs = new Map();
          const files = [];
          for (const [src, f] of FILES) {
            if (!src.startsWith(`${path}/`)) continue;
            const rest = src.slice(path.length + 1).split("/");
            if (rest.length === 1) files.push({ name: rest[0], path: src, bytes: f.size });
            else {
              const d = dirs.get(rest[0]) || { name: rest[0], path: `${path}/${rest[0]}`, audio_files: 0, has_subdirs: false };
              if (rest.length === 2) d.audio_files += 1;
              else d.has_subdirs = true;
              dirs.set(rest[0], d);
            }
          }
          if (!dirs.size && !files.length && path !== ROOT && !path.startsWith(ROOT)) {
            throw new Error(`${path}: no such folder.`);
          }
          return {
            path,
            parent: path === ROOT ? undefined : dirname(path),
            dirs: [...dirs.values()].sort((a, b) => a.name.localeCompare(b.name)),
            files: files.sort((a, b) => a.name.localeCompare(b.name)),
          };
        }
        case "preview":
          return preview(args.req);
        case "push": {
          if (!connected) throw new Error("No watch found. Connect it and try again.");
          const p = preview(args.req);
          runSeq += 1;
          const id = `demo-${runSeq}`;
          setTimeout(() => drive(id, p.files, 0, 29), 60);
          return { run_id: id };
        }
        case "stop":
          stopFlag = true;
          return null;
        case "watch_list":
          if (!connected) throw new Error("No watch found. Connect it and try again.");
          return JSON.parse(JSON.stringify(onWatch));
        case "ledger":
          return { path: "~/.local/share/pelican/ledger-3456789012.jsonl", rows: ledgerRows.slice() };
        default:
          throw new Error(`unknown command ${cmd}`);
      }
    }

    return {
      invoke,
      onProgress: (cb) => listeners.push(cb),
      onDrop: (cb) => {
        document.addEventListener("dragover", (e) => {
          e.preventDefault();
          cb("over");
        });
        document.addEventListener("dragleave", (e) => {
          if (!e.relatedTarget) cb("leave");
        });
        document.addEventListener("drop", (e) => {
          e.preventDefault();
          cb("leave");
          notice("In the demo a browser drop carries no file paths; the app receives them from the window.", { word: "Demo" });
        });
      },
      demo: {
        ROOT,
        SOT_DIR,
        MC_DIR,
        WR_DIR,
        FILES,
        setConnected: (c) => {
          connected = c;
        },
        preview,
        drive,
        fakeSha,
        slug,
      },
    };
  }

  // Jump to a state for screenshots: #watch, #nowatch, #choose, #library,
  // #review, #review-mix, #send, #done, #onwatch, #ledger.
  async function demoState(name) {
    const D = api.demo;
    S.run = null;
    S.mix = { on: false, name: "" };
    S.order = null;
    S.resend = false;
    S.overrides = { artist: "", album: "", genre: "", year: "" };
    S.preview = null;
    S.root = await api.invoke("library_root");
    D.setConnected(name !== "nowatch");
    S.status = await api.invoke("status");
    const albums = [
      { path: D.SOT_DIR, name: "Sea of Thieves", kind: "dir", count: 25 },
      { path: D.MC_DIR, name: basename(D.MC_DIR), kind: "dir", count: 13 },
    ];
    const tortuga = { path: `${D.WR_DIR}/32_Tortuga.WAV`, name: "32_Tortuga.WAV", kind: "file" };
    S.chosen = [];

    switch (name) {
      case "nowatch":
      case "watch":
        show("watch", { focus: false });
        break;
      case "choose":
        S.chosen = albums.slice();
        show("choose", { focus: false });
        break;
      case "library":
        S.chosen = [albums[0]];
        S.lib = await api.invoke("library_list", { path: S.root });
        show("library", { focus: false });
        break;
      case "review":
        S.chosen = [...albums, tortuga];
        S.preview = D.preview(previewReq());
        show("review", { focus: false });
        break;
      case "review-mix": {
        const pick = [
          `${D.SOT_DIR}/13 - Spectral Sails.wav`,
          [...D.FILES.keys()].find((k) => k.includes("/13 - Iva") && k.endsWith(".flac")),
          `${D.SOT_DIR}/01 - We Shall Sail Together.wav`,
          [...D.FILES.keys()].find((k) => k.includes("/05 - Iva")),
          `${D.WR_DIR}/19_Steel on Timber.WAV`,
          `${D.SOT_DIR}/21 - Strongholds Of The Sea.wav`,
          [...D.FILES.keys()].find((k) => k.includes("/10 - Iva")),
          `${D.SOT_DIR}/24 - The Wild Rose.wav`,
        ];
        S.chosen = pick.map((p) => ({ path: p, name: basename(p), kind: "file" }));
        S.mix = { on: true, name: "Long Run" };
        S.order = pick;
        S.preview = D.preview(previewReq());
        show("review", { focus: false });
        break;
      }
      case "send":
      case "done": {
        S.chosen = [tortuga, ...albums];
        const req = previewReq();
        S.preview = D.preview(req);
        const run = newRun(S.preview, req);
        run.id = "demo-roll";
        S.run = run;
        const upto = name === "done" ? run.lines.length : 14;
        let counter = 29;
        for (let i = 0; i < upto; i += 1) {
          const f = S.preview.files[i];
          const l = run.lines[i];
          l.shown = true;
          if (f.verdict === "skip") {
            l.state = "skipped";
            l.detail = f.reason;
            run.tally.skipped += 1;
          } else if (f.title === "Sunken Depths") {
            counter += 1;
            l.state = "failed";
            l.detail =
              "read-back mismatch: sent 4,948,096 bytes, read back 4,947,968; retry failed: the watch stopped answering (USB timeout)";
            run.tally.failed += 1;
          } else {
            counter += f.title === "Spectral Sails" ? 2 : 1;
            const remote = `pl${String(counter).padStart(5, "0")}-${D.slug(f.title)}.mp3`;
            l.state = "verified";
            l.sha = D.fakeSha(remote);
            l.detail = `sha256 ${l.sha.slice(0, 12)}`;
            run.tally.verified += 1;
          }
          run.current = i;
        }
        if (name === "done") {
          run.finished = { ...run.tally, stopped: false };
          show("done", { focus: false });
        } else {
          show("send", { focus: false });
          D.drive(run.id, S.preview.files, upto, counter, { ...run.tally });
        }
        Ink.resize();
        Ink.settle(Math.min(run.tally.verified, 5));
        break;
      }
      case "onwatch":
        show("onwatch", { focus: false });
        loadWatchRows();
        break;
      case "ledger":
        show("ledger", { focus: false });
        loadLedger();
        break;
      default:
        show("watch", { focus: false });
    }
    syncChrome();
  }

  boot();
})();
