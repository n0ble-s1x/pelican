/* Pelican demo: the window's IPC contract served from fixtures, so the
 * window runs in a plain browser with no watch. app.js loads this file only
 * outside the Tauri shell; the window is marked "Demo data".
 *
 * The URL hash jumps to a state: #watch, #nowatch, #permission, #wedged,
 * #library, #chosen, #playlist, #review, #review-playlist, #send, #done,
 * #send-wedged, #onwatch, #ledger, #reset-1 to #reset-4, #reset-refused,
 * #unnamed (a watch plugged in for the first time, not yet named),
 * #reset-done. Add ?still to settle every entrance for a screenshot.
 */
window.PelicanDemo = (app) => {
  "use strict";

  const { S, Ink, MAX_OBJECTS, RESERVE_BYTES, REPLUG } = app;
  const { basename, bytes, confirmClean, dirname, loadLedger, loadPlaces, loadWatchRows } = app;
  const { newRun, notice, previewReq, show, startBackup, syncChrome, turnBezel } = app;

  function mockApi() {
    const ROOT = "/mnt/share/Music";
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

    const MISMATCH =
      "read-back mismatch: sent 4,948,096 bytes, read back 4,947,968; the retry under a new name did not match either";
    const HOME = "/home/user";
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

    // The watch: Windrose 09-32 sent by Pelican (counters 2-25), two files
    // from other apps, and two stubs left by writes the firmware rejected.
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

    const DEMO_NAME = "Trail watch";
    const status = {
      connected: true,
      name: DEMO_NAME,
      model: "Forerunner 165 Music",
      serial: "3456789012",
      free_bytes: 2300000000,
      capacity_bytes: 3500000000,
      music_objects: onWatch.length,
      max_objects: MAX_OBJECTS,
      ledger: { verified: 24, failed: 0, unresolved: 0 },
    };

    let connected = true;
    let problem = "none"; // why the watch is unreachable: "none" | "permission" | "wedged"
    let wiped = false; // the demo watch has been factory-reset
    let ruleState = "current";
    // The shipped rule, as the shell compiles it in (udev/70-garmin-mtp.rules).
    const RULE_LINE = 'SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="091e", TAG+="uaccess"';
    const RULE_TEXT =
      "# Pelican: give the logged-in user direct USB access to Garmin watches.\n#\n" +
      "# (Demo: the comments of the shipped file are abridged here.)\n\n" +
      `${RULE_LINE}\n`;
    const SCRIPT =
      "install -m 644 /dev/stdin /etc/udev/rules.d/70-garmin-mtp.rules && udevadm control --reload && udevadm trigger --action=add --subsystem-match=usb --attr-match=idVendor=091e && udevadm settle";
    const ruleStatus = () => ({
      state: ruleState,
      path: ruleState === "missing" ? undefined : "/etc/udev/rules.d/70-garmin-mtp.rules",
      can_install: true,
      rule: RULE_TEXT,
      command: `/usr/bin/pkexec /bin/sh -c '${SCRIPT}'`,
      manual: `printf '%s\\n' '${RULE_LINE}' | sudo install -m 644 /dev/stdin /etc/udev/rules.d/70-garmin-mtp.rules && sudo udevadm control --reload && sudo udevadm trigger --action=add --subsystem-match=usb --attr-match=idVendor=091e`,
    });
    // Places outside the library.
    const ELSEWHERE = {
      "/": [["home", 0, true], ["mnt", 0, true], ["run", 0, true]],
      "/home": [["user", 0, true]],
      [HOME]: [["Documents", 0, true], ["Downloads", 0, false], ["Music", 0, false]],
      [`${HOME}/Documents`]: [["Pelican", 0, false]],
      [`${HOME}/Downloads`]: [],
      [`${HOME}/Music`]: [],
      "/mnt": [["share", 0, true]],
      "/mnt/share": [["Backups", 0, false], ["Music", 0, true]],
      "/run": [["media", 0, true]],
      "/run/media": [["user", 0, true]],
      "/run/media/user": [["USB", 0, false]],
      "/run/media/user/USB": [],
    };
    const PLACES = [
      { label: "Home", path: HOME, kind: "home" },
      { label: "Music", path: `${HOME}/Music`, kind: "music" },
      { label: "Library", path: ROOT, kind: "library" },
      { label: "share", path: "/mnt/share", kind: "network" },
      { label: "USB", path: "/run/media/user/USB", kind: "drive" },
    ];

    // The watch's GARMIN folder, as a backup lists it.
    const GARMIN = [];
    const day = (d) => `2026-${String(8 + Math.floor(d / 30)).padStart(2, "0")}-${String((d % 30) + 1).padStart(2, "0")}`;
    for (let d = 0; d < 48; d += 1) GARMIN.push([`GARMIN/Activity/${day(d)}-07-1${d % 10}-04.fit`, 180000 + ((d * 7919) % 90000)]);
    for (let d = 0; d < 56; d += 1) GARMIN.push([`GARMIN/Monitor/${day(d)}.fit`, 60000 + ((d * 3571) % 20000)]);
    for (let d = 0; d < 56; d += 1) GARMIN.push([`GARMIN/Sleep/${day(d)}.fit`, 9000 + ((d * 331) % 3000)]);
    for (let d = 0; d < 20; d += 1) GARMIN.push([`GARMIN/HRVStatus/${day(d)}.fit`, 2400]);
    GARMIN.push(["GARMIN/Metrics/Metrics.fit", 14200], ["GARMIN/Records/Records.fit", 3100], ["GARMIN/Settings/Settings.fit", 5200], ["GARMIN/Totals/Totals.fit", 900]);
    const backupListeners = [];
    const emitBackup = (ev) => backupListeners.forEach((cb) => cb(ev));
    let backupSeq = 0;
    async function backupRun(id, dest) {
      await wait(200);
      emitBackup({ run_id: id, kind: "listing" });
      await wait(500);
      let total = 0;
      let files = 0;
      stopFlag = false;
      for (let i = 0; i < GARMIN.length; i += 1) {
        const [path, size] = GARMIN[i];
        total += size;
        files += 1;
        emitBackup({ run_id: id, kind: "file", index: i, total: GARMIN.length, path, bytes: size });
        await wait(34);
        if (stopFlag) break;
      }
      const stopped = stopFlag && files < GARMIN.length;
      stopFlag = false;
      emitBackup({ run_id: id, kind: "finished", files, bytes: total, dest, failed: [], unreadable: 0, stopped });
    }

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
        } else if (!req.resend && !(req.resend_sources || []).includes(f.source) && verifiedKeys.has(key)) {
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
      } else if (objectsAfter > MAX_OBJECTS) {
        fits = { ok: false, free_bytes: status.free_bytes, objects_after: objectsAfter, reason: `That would make ${objectsAfter} tracks; the watch holds ${MAX_OBJECTS}.` };
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
          reason = MISMATCH;
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
          if (connected && wiped) {
            return { ...JSON.parse(JSON.stringify(status)), music_objects: 0, free_bytes: 3380000000, ledger: { verified: 0, failed: 0, unresolved: 0, resets: 1 } };
          }
          if (!connected && problem === "wedged") {
            return { connected: false, max_objects: MAX_OBJECTS, ledger: status.ledger, error_kind: "wedged", error: REPLUG };
          }
          return connected
            ? JSON.parse(JSON.stringify(status))
            : {
                connected: false,
                max_objects: MAX_OBJECTS,
                ledger: status.ledger,
                error_kind: problem === "permission" ? "permission" : "not_found",
                error:
                  problem === "permission"
                    ? "could not open the watch: Permission denied (os error 13). Pelican cannot open the watch without the udev rule: copy udev/70-garmin-mtp.rules to /etc/udev/rules.d/, run `sudo udevadm control --reload`, then unplug and replug the watch."
                    : "No watch found. Use a data cable (a charge-only cable carries nothing); if the watch is plugged in, install udev/70-garmin-mtp.rules once, and if the file manager opened it, release it with gio mount -u mtp://Garmin/",
              };
        case "udev_rule_status":
          return ruleStatus();
        case "install_udev_rule":
          // The real prompt is polkit's; the demo only waits as if for one.
          await wait(1400);
          ruleState = "current";
          return { outcome: "installed", message: "Installed /etc/udev/rules.d/70-garmin-mtp.rules and reloaded udev." };
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
          if (!path.startsWith(ROOT)) {
            const other = ELSEWHERE[path];
            if (!other) throw new Error(`${path}: no such folder.`);
            return { path, parent: dirname(path), dirs: other.map(([n, a, sub]) => ({ name: n, path: `${path === "/" ? "" : path}/${n}`, audio_files: a, has_subdirs: sub })), files: [] };
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
        case "places":
          return PLACES.map((p) => ({ ...p }));
        case "default_backup_dir":
          return `${HOME}/Documents/Pelican/${status.model} backup 2026-09-26`;
        case "backup_watch": {
          if (!connected) throw new Error("No watch found. Connect it and try again.");
          backupSeq += 1;
          const id = `demo-backup-${backupSeq}`;
          setTimeout(() => backupRun(id, args.dest), 20);
          return { run_id: id };
        }
        case "name_watch": {
          const n = String(args.name || "").trim();
          if (!n) throw "a device name cannot be empty";
          if ([...n].length > 40) throw `a device name can be at most 40 characters; this one is ${[...n].length}`;
          status.name = n;
          return n;
        }
        case "reset_check":
          if (!connected) throw new Error("No watch found. Connect it and try again.");
          return wiped ? { audio_objects: 0, clean: true } : { audio_objects: onWatch.length, clean: false };
        case "reset_ledger":
          if (!wiped) {
            return {
              clean: false,
              reset: false,
              audio_objects: onWatch.length,
              message: `The watch still has ${onWatch.length} song files in its music folder, so it has not been factory-reset. Pelican's record was left as it is. Reset the watch, plug it back in, and check again.`,
            };
          }
          if (!ledgerRows.some((r) => r.event === "reset")) {
            ledgerRows.push({ counter: 0, remote: "", event: "reset", at: new Date().toISOString(), reason: "factory reset confirmed: /Music read back with 0 audio objects" });
          }
          return { clean: true, reset: true, audio_objects: 0, message: "The watch is clean. Pelican has started a fresh record for it, so every song can be sent again." };
        default:
          throw new Error(`unknown command ${cmd}`);
      }
    }

    return {
      invoke,
      onProgress: (cb) => listeners.push(cb),
      onBackup: (cb) => backupListeners.push(cb),
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
        setProblem: (p, rule) => {
          problem = p;
          ruleState = rule;
        },
        setWiped: (w) => {
          wiped = w;
        },
        setNamed: (n) => {
          if (n) status.name = DEMO_NAME;
          else delete status.name;
        },
        MISMATCH,
        preview,
        drive,
        fakeSha,
        slug,
      },
    };
  }

  async function demoState(name) {
    const D = api.demo;
    S.run = null;
    S.mix = { on: false, name: "" };
    S.order = null;
    S.resend = false;
    S.resendSources = new Set();
    S.overrides = { artist: "", album: "", genre: "", year: "" };
    S.preview = null;
    S.reset = null;
    S.showChosen = false;
    D.setConnected(!["nowatch", "permission", "wedged"].includes(name));
    D.setProblem(
      name === "permission" ? "permission" : name === "wedged" ? "wedged" : "none",
      name === "permission" ? "missing" : "current",
    );
    D.setWiped(name === "reset-4" || name === "reset-done");
    D.setNamed(name !== "unnamed");
    S.ruleBusy = false;
    S.ruleInstalled = false;
    S.ruleMsg = null;
    S.status = await api.invoke("status");
    if (!S.status.connected) S.rule = await api.invoke("udev_rule_status");
    const albums = [
      { path: D.SOT_DIR, name: "Sea of Thieves", kind: "dir", count: 25 },
      { path: D.MC_DIR, name: basename(D.MC_DIR), kind: "dir", count: 13 },
    ];
    const file = (p) => ({ path: p, name: basename(p), kind: "file" });
    const tortuga = file(`${D.WR_DIR}/32_Tortuga.WAV`);
    const hearth = file(`${D.WR_DIR}/31_The Hearth.WAV`);
    // Songs selected across two folders, in the order they were selected.
    const mixPick = [
      `${D.SOT_DIR}/13 - Spectral Sails.wav`,
      `${D.WR_DIR}/19_Steel on Timber.WAV`,
      `${D.SOT_DIR}/01 - We Shall Sail Together.wav`,
      `${D.WR_DIR}/28_Starlight on a Swell.WAV`,
      `${D.SOT_DIR}/21 - Strongholds Of The Sea.wav`,
      `${D.SOT_DIR}/24 - The Wild Rose.wav`,
      `${D.WR_DIR}/10_Blow the Man Down.WAV`,
    ];
    S.chosen = [];

    switch (name) {
      case "nowatch":
      case "permission":
      case "wedged":
      case "watch":
      case "unnamed":
        show("watch", { focus: false });
        break;
      case "library":
      case "chosen": {
        S.chosen = mixPick.map(file);
        await loadPlaces();
        S.place = D.ROOT;
        S.open = new Set([D.SOT_DIR, D.WR_DIR]);
        for (const p of [D.ROOT, D.SOT_DIR, D.WR_DIR]) {
          S.tree.set(p, { state: "ok", l: await api.invoke("library_list", { path: p }) });
        }
        S.showChosen = name === "chosen";
        show("library", { focus: false });
        break;
      }
      case "playlist":
        S.chosen = mixPick.map(file);
        S.mix = { on: true, name: "" };
        show("playlist", { focus: false });
        break;
      case "review":
        S.chosen = [tortuga, hearth, ...albums];
        S.resendSources = new Set([hearth.path]);
        S.preview = D.preview(previewReq());
        show("review", { focus: false });
        break;
      case "review-playlist":
        S.chosen = mixPick.map(file);
        S.mix = { on: true, name: "Long Run" };
        S.order = mixPick;
        S.preview = D.preview(previewReq());
        show("review", { focus: false });
        break;
      case "send":
      case "send-wedged":
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
            l.detail = D.MISMATCH;
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
        turnBezel(run.tally.verified);
        if (name === "done") {
          run.finished = { ...run.tally, stopped: false };
          show("done", { focus: false });
        } else if (name === "send-wedged") {
          run.error = REPLUG;
          S.status = { connected: false, error_kind: "wedged", error: REPLUG, max_objects: MAX_OBJECTS, ledger: S.status.ledger };
          show("send", { focus: false });
        } else {
          show("send", { focus: false });
          D.drive(run.id, S.preview.files, upto, counter, { ...run.tally });
        }
        Ink.resize();
        Ink.settle(Math.min(run.tally.verified, 5), run.tally.failed);
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
      case "reset-1":
      case "reset-2":
      case "reset-3":
      case "reset-4": {
        const n = Number(name.slice(-1));
        S.reset = { step: n, reached: n, dest: null, backup: null, checking: false, found: null, outcome: null, error: null };
        if (n >= 2) S.reset.dest = await api.invoke("default_backup_dir");
        show("reset", { focus: false });
        if (n === 2) startBackup();
        break;
      }
      case "reset-refused":
        S.reset = { step: 4, reached: 4, dest: null, backup: null, checking: false, found: null, outcome: null, error: null };
        show("reset", { focus: false });
        await confirmClean();
        break;
      case "reset-done":
        S.reset = { step: 4, reached: 4, dest: null, backup: null, checking: false, found: null, outcome: null, error: null };
        show("reset", { focus: false });
        await confirmClean();
        break;
      default:
        show("watch", { focus: false });
    }
    syncChrome();
  }

  const api = mockApi();
  return { api, state: demoState };
};
