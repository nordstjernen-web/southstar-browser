// Southstar — the PDF viewer page: pdf.js renders each page to a canvas, with zoom, page navigation and find.
// Copyright 2026 Andreas Røsdal
// SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

import * as pdfjs from "../pdfjs/build/pdf.min.mjs";

const base = new URL("../pdfjs/", import.meta.url).href;
const expectedVersion = document.querySelector("meta[name=pdfjs-version]")?.content ?? "";

const ZOOM_STEPS = [0.25, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];
const PAGE_GAP = 12;
const RENDER_MARGIN = 1.5;

const $ = (id) => document.getElementById(id);
const pagesBox = $("pages");
const pageInput = $("page");
const pageCount = $("count");
const zoomSelect = $("zoom");
const findInput = $("find");
const findStatus = $("find-status");
const status = $("status");

const state = {
  doc: null,
  pages: [],
  zoom: "auto",
  scale: 1,
  queue: Promise.resolve(),
  texts: [],
  findFrom: { page: 0, index: -1 },
};

function setStatus(text) {
  status.textContent = text;
  status.hidden = !text;
}

function majorVersion(version) {
  return String(version ?? "").split(".")[0];
}

function compatiblePdfjs() {
  const found = majorVersion(pdfjs.version);
  return found !== "" && found === majorVersion(expectedVersion);
}

function showUnavailable() {
  const link = document.querySelector("a.download").cloneNode(true);
  link.className = "";
  link.textContent = "download";
  status.replaceChildren("PDF viewer unavailable — ", link);
  status.hidden = false;
  document.getElementById("toolbar").hidden = true;
}

function pdfBytes() {
  const holder = $("pdf-data");
  if (!holder) {
    return null;
  }
  const text = holder.textContent.trim();
  holder.remove();
  return Uint8Array.fromBase64(text);
}

function documentSource() {
  const bytes = pdfBytes();
  if (bytes) {
    return { data: bytes };
  }
  const file = new URLSearchParams(location.search).get("file");
  return file ? { url: new URL(file, location.href).href } : null;
}

function fitScale() {
  const first = state.pages[0];
  if (!first) {
    return 1;
  }
  const available = Math.max(200, pagesBox.clientWidth - 2 * PAGE_GAP);
  const fit = available / first.width;
  return state.zoom === "page-fit"
    ? Math.min(fit, Math.max(200, innerHeight - 60) / first.height)
    : Math.min(fit, 1.25);
}

function currentScale() {
  return typeof state.zoom === "number" ? state.zoom : fitScale();
}

function layoutPages() {
  state.scale = currentScale();
  for (const page of state.pages) {
    const w = Math.floor(page.width * state.scale);
    const h = Math.floor(page.height * state.scale);
    if (page.boxWidth !== w || page.boxHeight !== h) {
      page.boxWidth = w;
      page.boxHeight = h;
      page.box.style.width = w + "px";
      page.box.style.height = h + "px";
    }
    if (page.renderedScale !== state.scale) {
      page.stale = true;
    }
  }
  scheduleRender();
}

function visiblePages() {
  const top = pagesBox.scrollTop - innerHeight * RENDER_MARGIN;
  const bottom = pagesBox.scrollTop + pagesBox.clientHeight + innerHeight * RENDER_MARGIN;
  return state.pages.filter((page) => {
    const y = page.box.offsetTop;
    return y + page.box.offsetHeight >= top && y <= bottom;
  });
}

function scheduleRender() {
  state.queue = state.queue.then(renderVisible).catch((error) => console.error(error));
}

async function renderVisible() {
  for (const page of visiblePages()) {
    if (page.stale) {
      await renderPage(page);
    }
  }
}

async function renderPage(page) {
  page.stale = false;
  const scale = state.scale;
  page.renderedScale = scale;
  const ratio = window.devicePixelRatio || 1;
  const proxy = await state.doc.getPage(page.number);
  const viewport = proxy.getViewport({ scale: scale * ratio });
  const canvas = document.createElement("canvas");
  canvas.width = Math.floor(viewport.width);
  canvas.height = Math.floor(viewport.height);
  const task = proxy.render({ canvasContext: canvas.getContext("2d"), viewport });
  await task.promise;
  page.box.replaceChildren(canvas);
  if (state.scale !== scale) {
    page.stale = true;
  }
}

function currentPage() {
  const middle = pagesBox.scrollTop + pagesBox.clientHeight / 3;
  let found = state.pages[0];
  for (const page of state.pages) {
    if (page.box.offsetTop <= middle) {
      found = page;
    }
  }
  return found;
}

function goToPage(number) {
  const page = state.pages[Math.min(Math.max(number, 1), state.pages.length) - 1];
  if (page) {
    pagesBox.scrollTop = page.box.offsetTop - PAGE_GAP;
  }
}

function setZoom(zoom) {
  const anchor = currentPage();
  const offset = anchor ? (pagesBox.scrollTop - anchor.box.offsetTop) / state.scale : 0;
  state.zoom = zoom;
  layoutPages();
  if (anchor) {
    pagesBox.scrollTop = anchor.box.offsetTop + offset * state.scale;
  }
  zoomSelect.value = typeof zoom === "number" ? String(zoom) : zoom;
  if (zoomSelect.value === "") {
    zoomSelect.value = "custom";
    zoomSelect.querySelector("option[value=custom]").textContent = Math.round(zoom * 100) + "%";
  }
}

function stepZoom(direction) {
  const scale = state.scale;
  const next =
    direction > 0
      ? ZOOM_STEPS.find((step) => step > scale + 0.001)
      : ZOOM_STEPS.findLast((step) => step < scale - 0.001);
  if (next) {
    setZoom(next);
  }
}

async function pageText(index) {
  if (state.texts[index] === undefined) {
    const proxy = await state.doc.getPage(index + 1);
    const content = await proxy.getTextContent();
    state.texts[index] = content.items
      .map((item) => item.str + (item.hasEOL ? "\n" : ""))
      .join("")
      .toLowerCase();
  }
  return state.texts[index];
}

async function findNext(backwards) {
  const query = findInput.value.trim().toLowerCase();
  if (!query || !state.doc) {
    findStatus.textContent = "";
    return;
  }
  const count = state.pages.length;
  let { page, index } = state.findFrom;
  if (state.findFrom.query !== query) {
    page = currentPage().number - 1;
    index = backwards ? Infinity : -1;
  }
  for (let step = 0; step <= count; step++) {
    const text = await pageText(page);
    const at = backwards
      ? text.lastIndexOf(query, index === Infinity ? text.length : index - 1)
      : text.indexOf(query, index + 1);
    if (at >= 0 && !(backwards && index !== Infinity && at >= index)) {
      state.findFrom = { query, page, index: at };
      findStatus.textContent = "Page " + (page + 1);
      goToPage(page + 1);
      return;
    }
    page = (page + (backwards ? count - 1 : 1)) % count;
    index = backwards ? Infinity : -1;
  }
  state.findFrom = { query, page: 0, index: -1 };
  findStatus.textContent = "Not found";
}

function updatePageNumber() {
  const page = currentPage();
  const text = page ? String(page.number) : "";
  if (document.activeElement !== pageInput && pageInput.value !== text) {
    pageInput.value = text;
  }
}

function bindControls() {
  pagesBox.addEventListener("scroll", () => {
    updatePageNumber();
    scheduleRender();
  });
  let width = innerWidth;
  addEventListener("resize", () => {
    if (innerWidth !== width && typeof state.zoom !== "number") {
      width = innerWidth;
      layoutPages();
    } else {
      scheduleRender();
    }
  });
  pageInput.addEventListener("change", () => goToPage(parseInt(pageInput.value, 10) || 1));
  $("prev").addEventListener("click", () => goToPage(currentPage().number - 1));
  $("next").addEventListener("click", () => goToPage(currentPage().number + 1));
  $("zoom-out").addEventListener("click", () => stepZoom(-1));
  $("zoom-in").addEventListener("click", () => stepZoom(1));
  zoomSelect.addEventListener("change", () => {
    const value = zoomSelect.value;
    if (value !== "custom") {
      setZoom(value === "auto" || value === "page-fit" ? value : Number(value));
    }
  });
  findInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      findNext(event.shiftKey);
    } else if (event.key === "Escape") {
      findInput.value = "";
      findStatus.textContent = "";
      findInput.blur();
    }
  });
  addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === "f") {
      event.preventDefault();
      findInput.focus();
      findInput.select();
    } else if ((event.ctrlKey || event.metaKey) && (event.key === "+" || event.key === "=")) {
      event.preventDefault();
      stepZoom(1);
    } else if ((event.ctrlKey || event.metaKey) && event.key === "-") {
      event.preventDefault();
      stepZoom(-1);
    }
  });
}

async function open() {
  const source = documentSource();
  if (!source) {
    setStatus("No PDF document was given.");
    return;
  }
  setStatus("Loading…");
  const task = pdfjs.getDocument({
    ...source,
    cMapUrl: base + "cmaps/",
    cMapPacked: true,
    standardFontDataUrl: base + "standard_fonts/",
    wasmUrl: base + "wasm/",
    iccUrl: base + "iccs/",
    useWorkerFetch: false,
    useSystemFonts: false,
    isEvalSupported: false,
    enableScripting: false,
    isOffscreenCanvasSupported: false,
    isImageDecoderSupported: false,
  });
  task.onPassword = (update, reason) => {
    const prompt = reason === pdfjs.PasswordResponses.INCORRECT_PASSWORD
      ? "Incorrect password. Enter the password to open this PDF:"
      : "Enter the password to open this PDF:";
    const password = window.prompt(prompt);
    if (password === null) {
      setStatus("This PDF is password protected.");
    } else {
      update(password);
    }
  };
  const doc = await task.promise;
  state.doc = doc;
  pageCount.textContent = String(doc.numPages);
  pageInput.max = String(doc.numPages);
  for (let number = 1; number <= doc.numPages; number++) {
    const proxy = await doc.getPage(number);
    const viewport = proxy.getViewport({ scale: 1 });
    const box = document.createElement("div");
    box.className = "page";
    box.dataset.page = String(number);
    pagesBox.append(box);
    state.pages.push({ number, box, width: viewport.width, height: viewport.height, stale: true });
    if (number === 1) {
      layoutPages();
      setStatus("");
    }
  }
  layoutPages();
  const requested = /(?:^|&)page=(\d+)/.exec(location.hash.slice(1));
  if (requested) {
    goToPage(Number(requested[1]));
  }
  updatePageNumber();
  const { info } = await doc.getMetadata().catch(() => ({ info: null }));
  if (info?.Title && String(info.Title).trim()) {
    document.title = String(info.Title).trim();
  }
}

if (!compatiblePdfjs()) {
  console.error("pdf.js " + pdfjs.version + " does not match southstar-pdfjs " + expectedVersion);
  showUnavailable();
} else {
  pdfjs.GlobalWorkerOptions.workerSrc = base + "build/pdf.worker.min.mjs";
  bindControls();
  open().catch((error) => {
    console.error(error);
    setStatus("This PDF could not be displayed: " + (error?.message || error));
  });
}
