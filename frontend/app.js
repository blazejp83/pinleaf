// Pinleaf note window: Markdown rendering + view/edit toggle + on-note settings popover.
const { invoke } = window.__TAURI__.core;

const PALETTE = [
  0xffdd5c, 0xffb3ba, 0xffd8a8, 0xbaffc9, 0xbae1ff, 0xe6ccff, 0xd7d7d7, 0xffffff,
];
const SKINS = [
  { id: "classic", label: "Classic" },
  { id: "ruled", label: "Ruled" },
  { id: "grid", label: "Grid" },
  { id: "gradient", label: "Gradient" },
  { id: "dark", label: "Dark" },
];
function applySkin(skin) {
  document.documentElement.dataset.skin = skin || "classic";
}

let current = null; // { id, title, body, color, due_ms, tags }
let mode = "view"; // "view" | "edit"
let saveTimer = null;

const $ = (id) => document.getElementById(id);
const colorHex = (n) => "#" + (n & 0xffffff).toString(16).padStart(6, "0");

// ---------- Markdown ----------
function escapeHtml(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function sanitizeUrl(u) {
  return /^(https?:\/\/|mailto:)/i.test(u) ? u.replace(/"/g, "%22") : null;
}
function sanitizeImgUrl(u) {
  return /^(pinleaf:\/\/|https:\/\/|data:image\/)/i.test(u) ? u.replace(/"/g, "%22") : null;
}
// Inline formatting on already-escaped text.
function inline(s) {
  s = s.replace(/`([^`]+)`/g, (m, c) => `<code>${c}</code>`);
  // images (before links: ![alt](url) contains [alt](url))
  s = s.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (m, alt, u) => {
    const pin = u.match(/^pinleaf:\/\/object\/(.+)$/i);
    if (pin) return `<img data-obj="${pin[1]}" alt="${alt}">`; // resolved async via get_image
    const safe = sanitizeImgUrl(u);
    return safe ? `<img src="${safe}" alt="${alt}">` : "";
  });
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (m, t, u) => {
    const safe = sanitizeUrl(u);
    return safe ? `<a data-href="${safe}">${t}</a>` : t;
  });
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  s = s.replace(/__([^_]+)__/g, "<strong>$1</strong>");
  s = s.replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
  s = s.replace(/~~([^~]+)~~/g, "<del>$1</del>");
  s = s.replace(/==([^=]+)==/g, "<mark>$1</mark>");
  return s;
}
function renderMarkdown(src) {
  const lines = escapeHtml(src).split(/\r?\n/);
  let html = "";
  let i = 0;
  let taskIndex = 0;
  const isBlockStart = (l) =>
    /^(#{1,6}\s|>|\s*[-*]\s|\s*\d+\.\s|```|---|\*\*\*|___)/.test(l);

  while (i < lines.length) {
    const line = lines[i];

    if (/^```/.test(line)) {
      const buf = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) buf.push(lines[i++]);
      i++;
      html += `<pre><code>${buf.join("\n")}</code></pre>`;
      continue;
    }
    if (/^(---|\*\*\*|___)\s*$/.test(line)) {
      html += "<hr>";
      i++;
      continue;
    }
    const h = line.match(/^(#{1,6})\s+(.*)$/);
    if (h) {
      const lvl = h[1].length;
      html += `<h${lvl}>${inline(h[2])}</h${lvl}>`;
      i++;
      continue;
    }
    // GitHub-style pipe table: a header row followed by a |---|:--:|---| separator.
    if (
      line.includes("|") &&
      i + 1 < lines.length &&
      /^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)+\|?\s*$/.test(lines[i + 1])
    ) {
      const splitRow = (l) => {
        let c = l.trim();
        if (c.startsWith("|")) c = c.slice(1);
        if (c.endsWith("|")) c = c.slice(0, -1);
        return c.split("|").map((x) => x.trim());
      };
      const headers = splitRow(line);
      const aligns = splitRow(lines[i + 1]).map((a) => {
        const l = a.startsWith(":"), r = a.endsWith(":");
        return l && r ? "center" : r ? "right" : l ? "left" : "";
      });
      i += 2;
      const rows = [];
      while (i < lines.length && lines[i].includes("|") && !/^\s*$/.test(lines[i])) {
        rows.push(splitRow(lines[i]));
        i++;
      }
      const cell = (tag, txt, ci) => {
        const a = aligns[ci] ? ` style="text-align:${aligns[ci]}"` : "";
        return `<${tag}${a}>${inline(txt || "")}</${tag}>`;
      };
      let t = "<table><thead><tr>";
      headers.forEach((hh, ci) => (t += cell("th", hh, ci)));
      t += "</tr></thead><tbody>";
      rows.forEach((r) => {
        t += "<tr>";
        headers.forEach((_, ci) => (t += cell("td", r[ci], ci)));
        t += "</tr>";
      });
      t += "</tbody></table>";
      html += t;
      continue;
    }
    if (/^>\s?/.test(line)) {
      const buf = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) buf.push(lines[i++].replace(/^>\s?/, ""));
      html += `<blockquote>${inline(buf.join("<br>"))}</blockquote>`;
      continue;
    }
    if (/^\s*[-*]\s+/.test(line)) {
      html += "<ul>";
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) {
        const tm = lines[i].match(/^\s*[-*]\s+\[([ xX])\]\s+(.*)$/);
        if (tm) {
          const checked = tm[1].toLowerCase() === "x" ? "checked" : "";
          html += `<li class="task"><input type="checkbox" data-ti="${taskIndex}" ${checked}> ${inline(tm[2])}</li>`;
          taskIndex++;
        } else {
          html += `<li>${inline(lines[i].match(/^\s*[-*]\s+(.*)$/)[1])}</li>`;
        }
        i++;
      }
      html += "</ul>";
      continue;
    }
    if (/^\s*\d+\.\s+/.test(line)) {
      html += "<ol>";
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
        html += `<li>${inline(lines[i].match(/^\s*\d+\.\s+(.*)$/)[1])}</li>`;
        i++;
      }
      html += "</ol>";
      continue;
    }
    if (/^\s*$/.test(line)) {
      i++;
      continue;
    }
    // paragraph
    const buf = [line];
    i++;
    while (i < lines.length && !/^\s*$/.test(lines[i]) && !isBlockStart(lines[i])) buf.push(lines[i++]);
    html += `<p>${inline(buf.join("<br>"))}</p>`;
  }
  return html || '<p class="empty">Double-click to edit…</p>';
}

// ---------- meta / title ----------
function formatDue(ms) {
  if (ms == null) return null;
  return new Date(ms).toLocaleString(undefined, {
    weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });
}
function toInputValue(ms) {
  const d = new Date(ms);
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}
function applyColor(n) {
  document.documentElement.style.setProperty("--note", colorHex(n));
}
function renderTitle() {
  const el = $("title");
  // Show the title field when a title is set, or while editing the note (so you can add one).
  const show = !!(current.title && current.title.length) || mode === "edit";
  el.hidden = !show;
  if (show && document.activeElement !== el) el.value = current.title || "";
}
async function saveTitle(v) {
  current.title = v.trim() ? v.trim() : null;
  try {
    await invoke("set_title", { title: v });
  } catch (e) {
    console.error("set_title", e);
  }
}
function renderMeta() {
  const due = formatDue(current.due_ms);
  const dueEl = $("due");
  if (due) {
    dueEl.textContent = "⏰ " + due;
    dueEl.classList.remove("overdue", "soon");
    const delta = current.due_ms - Date.now();
    if (delta < 0) dueEl.classList.add("overdue");
    else if (delta < 86_400_000) dueEl.classList.add("soon");
    dueEl.hidden = false;
  } else {
    dueEl.hidden = true;
  }
  const tagsEl = $("tags");
  tagsEl.innerHTML = "";
  for (const t of current.tags) {
    const chip = document.createElement("span");
    chip.className = "tag-chip";
    chip.textContent = "#" + t;
    tagsEl.appendChild(chip);
  }
  $("meta").hidden = !due && current.tags.length === 0;
}

// ---------- body view/edit ----------
// ---------- roll-up ----------
function applyRoll(rolled) {
  $("note").classList.toggle("rolled", !!rolled);
  const b = $("roll");
  b.textContent = rolled ? "▴" : "▾";
  b.title = rolled ? "Roll down" : "Roll up";
  b.classList.toggle("active", !!rolled);
}
async function setRolled(v) {
  current.rolled = v;
  applyRoll(v);
  try {
    await invoke("set_rolled", { rolled: v }); // backend resizes the window
  } catch (e) {
    console.error("set_rolled", e);
  }
}

// ---------- note flags (star / completed / stay-on-top / transparency) ----------
function applyFlags() {
  const star = $("star");
  star.textContent = current.starred ? "★" : "☆";
  star.classList.toggle("active", !!current.starred);
  $("pin").classList.toggle("active", !!current.stay_on_top);
  $("note").classList.toggle("completed", !!current.completed);
  const t = current.transparency == null ? 255 : current.transparency;
  $("note").style.opacity = String(Math.max(0.2, t / 255));
}
async function setStarred(v) {
  current.starred = v;
  applyFlags();
  try { await invoke("set_starred", { starred: v }); } catch (e) { console.error("set_starred", e); }
}
async function setStayOnTop(v) {
  current.stay_on_top = v;
  applyFlags();
  try { await invoke("set_stay_on_top", { on: v }); } catch (e) { console.error("set_stay_on_top", e); }
}
async function setCompleted(v) {
  current.completed = v;
  applyFlags();
  try { await invoke("set_completed", { completed: v }); } catch (e) { console.error("set_completed", e); }
}
let transpTimer = null;
function setTransparency(pct) {
  current.transparency = Math.round((pct / 100) * 255);
  applyFlags();
  clearTimeout(transpTimer);
  transpTimer = setTimeout(() => {
    invoke("set_transparency", { value: current.transparency }).catch((e) => console.error("set_transparency", e));
  }, 300);
}

function renderBody() {
  const r = $("rendered");
  r.innerHTML = renderMarkdown(current.body || "");
  r.querySelectorAll("input[type=checkbox][data-ti]").forEach((cb) => {
    cb.addEventListener("change", () => toggleTask(parseInt(cb.dataset.ti, 10)));
  });
  r.querySelectorAll("a[data-href]").forEach((a) => {
    a.addEventListener("click", (e) => {
      e.preventDefault();
      invoke("open_url", { url: a.dataset.href }).catch((err) => console.error("open_url", err));
    });
  });
  r.querySelectorAll("img[data-obj]").forEach(async (img) => {
    try {
      img.src = await invoke("get_image", { id: img.dataset.obj });
    } catch (e) {
      console.error("get_image", e);
    }
  });
}
function toggleTask(n) {
  const lines = current.body.split(/\r?\n/);
  let k = -1;
  for (let j = 0; j < lines.length; j++) {
    const m = lines[j].match(/^(\s*[-*]\s+\[)([ xX])(\]\s+.*)$/);
    if (m) {
      k++;
      if (k === n) {
        lines[j] = m[1] + (m[2].toLowerCase() === "x" ? " " : "x") + m[3];
        break;
      }
    }
  }
  current.body = lines.join("\n");
  renderBody();
  saveNow();
}
function enterEdit() {
  mode = "edit";
  const b = $("body");
  b.value = current.body;
  $("rendered").hidden = true;
  $("toolbar").hidden = false;
  b.hidden = false;
  renderTitle(); // reveal the title field for editing
  b.focus();
  b.setSelectionRange(b.value.length, b.value.length);
}
function exitEdit() {
  if (mode !== "edit") return;
  current.body = $("body").value;
  mode = "view";
  $("body").hidden = true;
  $("toolbar").hidden = true;
  $("rendered").hidden = false;
  renderBody();
  renderTitle(); // hide the title field again if no title was set
  saveNow();
}

// ---------- edit-mode formatting ----------
function syncBody() {
  current.body = $("body").value;
  scheduleSave();
}
function surround(before, after) {
  const b = $("body");
  const s = b.selectionStart, e = b.selectionEnd, v = b.value;
  const sel = v.slice(s, e);
  b.value = v.slice(0, s) + before + sel + after + v.slice(e);
  b.setSelectionRange(s + before.length, s + before.length + sel.length);
  syncBody();
}
function prefixLines(prefix) {
  const b = $("body");
  const s = b.selectionStart, e = b.selectionEnd, v = b.value;
  const ls = v.lastIndexOf("\n", s - 1) + 1;
  let le = v.indexOf("\n", e);
  if (le < 0) le = v.length;
  const block = v.slice(ls, le).split("\n").map((l) => prefix + l).join("\n");
  b.value = v.slice(0, ls) + block + v.slice(le);
  b.setSelectionRange(ls, ls + block.length);
  syncBody();
}
function insertLink() {
  const b = $("body");
  const s = b.selectionStart, e = b.selectionEnd, v = b.value;
  const sel = v.slice(s, e) || "text";
  const md = `[${sel}](https://)`;
  b.value = v.slice(0, s) + md + v.slice(e);
  const urlPos = s + sel.length + 3; // inside (...)
  b.setSelectionRange(urlPos, urlPos + "https://".length);
  syncBody();
}
function insertTable() {
  insertText("\n| Column | Column |\n| --- | --- |\n| Cell | Cell |\n");
}
function insertText(text) {
  const b = $("body");
  const s = b.selectionStart, v = b.value;
  b.value = v.slice(0, s) + text + v.slice(b.selectionEnd);
  const pos = s + text.length;
  b.setSelectionRange(pos, pos);
  syncBody();
}
function readAsBase64(file) {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(String(r.result).split(",")[1]);
    r.onerror = rej;
    r.readAsDataURL(file);
  });
}
const TOOLBAR_ACTIONS = {
  bold: () => surround("**", "**"),
  italic: () => surround("*", "*"),
  strike: () => surround("~~", "~~"),
  mark: () => surround("==", "=="),
  code: () => surround("`", "`"),
  link: insertLink,
  h: () => prefixLines("# "),
  ul: () => prefixLines("- "),
  task: () => prefixLines("- [ ] "),
  table: insertTable,
  image: () => $("file-input").click(),
};
function ensureEditThenInsert(text) {
  if (mode !== "edit") enterEdit();
  insertText(text);
}
function saveNow() {
  clearTimeout(saveTimer);
  invoke("save_body", { body: current.body }).catch((e) => console.error("save_body", e));
}
function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(saveNow, 500);
}

// ---------- popover (color / due / tags / delete) ----------
function renderSwatches() {
  const box = $("swatches");
  box.innerHTML = "";
  for (const c of PALETTE) {
    const b = document.createElement("button");
    b.className = "swatch" + (c === current.color ? " active" : "");
    b.style.background = colorHex(c);
    b.addEventListener("click", () => setColor(c));
    box.appendChild(b);
  }
}
function renderSkins() {
  const box = $("skins");
  box.innerHTML = "";
  const activeSkin = current.skin || "classic";
  for (const sk of SKINS) {
    const b = document.createElement("button");
    b.className = "skin-btn" + (sk.id === activeSkin ? " active" : "");
    b.textContent = sk.label;
    b.addEventListener("click", () => setSkin(sk.id));
    box.appendChild(b);
  }
}
async function setSkin(id) {
  current.skin = id;
  applySkin(id);
  renderSkins();
  try { await invoke("set_skin", { skin: id }); } catch (e) { console.error("set_skin", e); }
}
function renderChipsEditor() {
  const box = $("chips");
  box.innerHTML = "";
  current.tags.forEach((t, i) => {
    const chip = document.createElement("span");
    chip.className = "edit-chip";
    chip.textContent = "#" + t;
    const x = document.createElement("button");
    x.textContent = "×";
    x.addEventListener("click", () => {
      const next = current.tags.slice();
      next.splice(i, 1);
      setTags(next);
    });
    chip.appendChild(x);
    box.appendChild(chip);
  });
}
function refreshPopover() {
  renderSwatches();
  renderSkins();
  $("due-input").value = current.due_ms != null ? toInputValue(current.due_ms) : "";
  $("repeat-input").value = current.repeat || "none";
  updateSnoozeVisibility();
  $("completed").checked = !!current.completed;
  $("transparency").value = String(Math.round(((current.transparency == null ? 255 : current.transparency) / 255) * 100));
  renderChipsEditor();
}
async function setColor(c) {
  current.color = c;
  applyColor(c);
  renderSwatches();
  try { await invoke("set_color", { color: c }); } catch (e) { console.error("set_color", e); }
}
function updateSnoozeVisibility() {
  $("snooze").hidden = !(current.due_ms != null && current.due_ms < Date.now());
}
async function saveDue() {
  const v = $("due-input").value;
  const ms = v ? new Date(v).getTime() : null;
  const repeat = ms ? $("repeat-input").value : "none";
  current.due_ms = ms;
  current.repeat = repeat;
  renderMeta();
  updateSnoozeVisibility();
  try { await invoke("set_due", { due: ms, repeat }); } catch (e) { console.error("set_due", e); }
}
async function snoozeNote(minutes) {
  try {
    const when = await invoke("snooze", { minutes });
    current.due_ms = when;
    renderMeta();
    $("due-input").value = toInputValue(when);
    updateSnoozeVisibility();
  } catch (e) { console.error("snooze", e); }
}
async function setTags(arr) {
  current.tags = arr;
  renderMeta();
  renderChipsEditor();
  try { await invoke("set_tags", { tags: arr }); } catch (e) { console.error("set_tags", e); }
}

// ---------- boot ----------
async function loadNote() {
  current = await invoke("load_note");
  applyColor(current.color);
  applySkin(current.skin);
  applyFlags();
  applyRoll(current.rolled);
  renderTitle();
  renderMeta();
  renderBody();
  if (!current.body || current.body.trim() === "") enterEdit(); // new/empty notes start editable
}

window.addEventListener("DOMContentLoaded", () => {
  const body = $("body");
  body.addEventListener("input", () => {
    current.body = body.value;
    scheduleSave();
  });
  body.addEventListener("blur", (e) => {
    // Stay in edit mode when focus moves to the title field or the toolbar; only exit when
    // focus actually leaves the note's editing controls.
    const to = e.relatedTarget;
    if (to && (to.id === "title" || (to.closest && to.closest("#toolbar")))) return;
    exitEdit();
  });
  body.addEventListener("paste", async (e) => {
    const items = e.clipboardData && e.clipboardData.items;
    if (!items) return;
    for (const it of items) {
      if (it.type && it.type.startsWith("image/")) {
        e.preventDefault();
        const file = it.getAsFile();
        try {
          const b64 = await readAsBase64(file);
          const ref = await invoke("insert_image", { b64, mime: file.type });
          insertText(`\n![](${ref})\n`);
        } catch (err) {
          console.error("insert_image", err);
        }
        break;
      }
    }
  });
  body.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      exitEdit();
    }
  });

  $("rendered").addEventListener("dblclick", (e) => {
    if (e.target.closest("input, a")) return; // don't edit when toggling a checkbox / clicking a link
    enterEdit();
  });

  // Title field: live-update current, save on blur/Enter, Enter jumps to the body.
  $("title").addEventListener("input", (e) => {
    current.title = e.target.value;
  });
  $("title").addEventListener("change", (e) => saveTitle(e.target.value));
  $("title").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      saveTitle(e.target.value);
      enterEdit();
    }
  });

  // Toolbar: mousedown + preventDefault keeps the textarea focused/selected during the action.
  $("toolbar").addEventListener("mousedown", (e) => {
    const btn = e.target.closest("button");
    if (!btn) return;
    e.preventDefault();
    const act = TOOLBAR_ACTIONS[btn.dataset.act];
    if (act) act();
  });

  // File-picker insert (toolbar image button).
  $("file-input").addEventListener("change", async (e) => {
    const file = e.target.files && e.target.files[0];
    e.target.value = "";
    if (!file || !file.type.startsWith("image/")) return;
    try {
      const b64 = await readAsBase64(file);
      const ref = await invoke("insert_image", { b64, mime: file.type });
      ensureEditThenInsert(`\n![](${ref})\n`);
    } catch (err) {
      console.error("insert_image", err);
    }
  });

  // Drag-drop image files onto THIS note. Scoped to the current webview so a drop only
  // affects the window it landed on (a global event listener fires in every note window).
  const wv = window.__TAURI__ && window.__TAURI__.webview;
  if (wv && wv.getCurrentWebview) {
    wv.getCurrentWebview().onDragDropEvent(async (event) => {
      const pl = event.payload;
      if (!pl || pl.type !== "drop") return;
      for (const p of pl.paths || []) {
        if (!/\.(png|jpe?g|gif|webp|bmp)$/i.test(p)) continue;
        try {
          const ref = await invoke("insert_image_path", { path: p });
          ensureEditThenInsert(`\n![](${ref})\n`);
        } catch (err) {
          console.error("insert_image_path", err);
        }
      }
    });
  }

  $("gear").addEventListener("click", (e) => {
    e.stopPropagation();
    const pop = $("popover");
    pop.hidden = !pop.hidden;
    if (!pop.hidden) refreshPopover();
  });
  $("roll").addEventListener("click", () => setRolled(!current.rolled));
  $("star").addEventListener("click", () => setStarred(!current.starred));
  $("pin").addEventListener("click", () => setStayOnTop(!current.stay_on_top));
  $("completed").addEventListener("change", (e) => setCompleted(e.target.checked));
  $("transparency").addEventListener("input", (e) => setTransparency(parseInt(e.target.value, 10)));
  $("close").addEventListener("click", () => invoke("close_note"));
  $("delete").addEventListener("click", () => {
    invoke("delete_note").catch((e) => console.error("delete_note", e));
  });

  $("popover").addEventListener("click", (e) => e.stopPropagation());
  document.addEventListener("click", () => {
    $("popover").hidden = true;
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !$("popover").hidden) {
      $("popover").hidden = true;
    }
  });

  $("due-input").addEventListener("change", saveDue);
  $("repeat-input").addEventListener("change", saveDue);
  $("due-clear").addEventListener("click", () => {
    $("due-input").value = "";
    $("repeat-input").value = "none";
    saveDue();
  });
  $("snooze").addEventListener("click", () => snoozeNote(10));
  $("tag-input").addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    const name = e.target.value.trim();
    e.target.value = "";
    if (name && !current.tags.includes(name)) setTags(current.tags.concat(name));
  });

  loadNote().catch((e) => {
    $("rendered").textContent = "Error loading note: " + e;
  });
});
