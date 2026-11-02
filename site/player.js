// Dussek's website: one player for the page (a play button, a piece's name, a feature's step), its
// bar with the position to seek, the download's system, the entrances. Without it the page still
// works: each play control is a link the browser plays, and the video and the download links are plain.
(function () {
  "use strict";
  document.documentElement.classList.add("js");
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- the player
  var audio = new Audio();
  audio.preload = "none";
  var current = null; // the pressed control
  var bar = document.createElement("div");
  bar.className = "now";
  bar.setAttribute("role", "region");
  bar.setAttribute("aria-label", "Now playing");
  bar.innerHTML = '<span class="now-title"></span>' +
    '<input class="now-seek" type="range" min="0" max="0" step="0.1" value="0" aria-label="Position" disabled>' +
    '<span class="now-time">0:00</span><button type="button">Pause</button>';
  document.body.appendChild(bar);
  var barTitle = bar.querySelector(".now-title");
  var barSeek = bar.querySelector(".now-seek");
  var barTime = bar.querySelector(".now-time");
  var barButton = bar.querySelector("button");
  // out of the tab order and the reading until it first shows
  bar.inert = true;
  bar.setAttribute("aria-hidden", "true");
  barButton.tabIndex = -1;
  barSeek.tabIndex = -1;
  function showBar() {
    if (bar.classList.contains("on")) return;
    bar.classList.add("on");
    bar.inert = false;
    bar.removeAttribute("aria-hidden");
    barButton.removeAttribute("tabindex");
    barSeek.removeAttribute("tabindex");
    // the page's end clear of it (the footer's links on a phone)
    document.documentElement.style.setProperty("--now-h", bar.offsetHeight + 16 + "px");
    document.documentElement.classList.add("playing");
  }

  function mark(control, on) {
    if (control) control.setAttribute("aria-pressed", on ? "true" : "false");
  }
  function clock(t) {
    t = Math.max(0, Math.floor(t || 0));
    return Math.floor(t / 60) + ":" + String(t % 60).padStart(2, "0");
  }
  // another control (a feature's other step among them) plays its clip from the start; the same
  // one pauses, and resumes where it paused
  function play(control) {
    if (!canOpus) { current = control; cannot("This browser cannot play the clips (Ogg Opus): a newer one can."); return; }
    var src = control.getAttribute("data-src");
    if (current === control && !audio.paused) {
      audio.pause();
      return;
    }
    if (current !== control) {
      if (current) mark(current, false);
      current = control;
      if (audio.getAttribute("src") !== src) {
        audio.src = src;
        showTime(0, NaN);
      } else {
        audio.currentTime = 0;
      }
    }
    barTitle.textContent = control.getAttribute("data-title");
    showBar();
    audio.play().catch(function () {});
  }
  audio.addEventListener("play", function () { mark(current, true); barButton.textContent = "Pause"; });
  audio.addEventListener("pause", function () { mark(current, false); barButton.textContent = "Play"; });
  audio.addEventListener("ended", function () { mark(current, false); barButton.textContent = "Play"; });

  // the position: the time, and the slider that seeks (drag, click, the arrow keys); the length once
  // known (a server that does not say a file's size leaves it unknown while it streams, the slider off)
  var dragging = false;
  function showTime(t, d) {
    var known = isFinite(d) && d > 0;
    barTime.textContent = clock(t) + (known ? " / " + clock(d) : "");
    barSeek.disabled = !known;
    barSeek.max = known ? d : 0;
    if (!dragging) barSeek.value = known ? Math.min(t, d) : 0;
    barSeek.style.setProperty("--p", (known ? Math.min(100, (100 * t) / d) : 0) + "%");
    barSeek.setAttribute("aria-valuetext", clock(t) + (known ? " of " + clock(d) : ""));
  }
  ["timeupdate", "durationchange", "loadedmetadata", "seeked"].forEach(function (name) {
    audio.addEventListener(name, function () { showTime(audio.currentTime, audio.duration); });
  });
  function seek(t) {
    if (!isFinite(audio.duration)) return;
    t = Math.min(Math.max(0, t), audio.duration);
    audio.currentTime = t;
    showTime(t, audio.duration);
  }
  barSeek.addEventListener("pointerdown", function () { dragging = true; });
  ["pointerup", "pointercancel", "change"].forEach(function (name) {
    barSeek.addEventListener(name, function () { dragging = false; });
  });
  barSeek.addEventListener("input", function () { seek(parseFloat(barSeek.value)); });
  // the arrow keys step 5 s (Page Up and Down 30 s), not the slider's 0.1 s; Home and End as they are
  barSeek.addEventListener("keydown", function (e) {
    var by = { ArrowLeft: -5, ArrowDown: -5, ArrowRight: 5, ArrowUp: 5, PageDown: -30, PageUp: 30 }[e.key];
    if (by === undefined) return;
    e.preventDefault();
    seek(audio.currentTime + by);
  });
  // a browser without Ogg Opus (older Safari) says so instead of seeming to play
  var canOpus = audio.canPlayType('audio/ogg; codecs="opus"') !== "";
  function cannot(text) {
    mark(current, false);
    barTitle.textContent = text;
    barTime.textContent = "";
    barButton.textContent = "Play";
    showBar();
  }
  audio.addEventListener("error", function () {
    cannot(canOpus ? "This clip did not load." : "This browser cannot play the clips (Ogg Opus): a newer one can.");
  });
  barButton.addEventListener("click", function () {
    if (audio.paused) audio.play().catch(function () {}); else audio.pause();
  });

  document.addEventListener("click", function (e) {
    var b = e.target.closest("[data-src]");
    if (!b) return;
    e.preventDefault();
    play(b);
  });
  // the links to the clips become toggle buttons: pressed while theirs plays, Space plays as Enter does
  document.querySelectorAll("[data-src]").forEach(function (b) {
    b.setAttribute("role", "button");
    b.setAttribute("aria-pressed", "false");
  });
  document.addEventListener("keydown", function (e) {
    if (e.key !== " " || !e.target.closest) return;
    var b = e.target.closest("[data-src]");
    if (b) { e.preventDefault(); b.click(); }
  });
  // the video and the page's audio: one at a time
  document.querySelectorAll("video").forEach(function (v) {
    v.addEventListener("play", function () { audio.pause(); });
  });
  audio.addEventListener("play", function () {
    document.querySelectorAll("video").forEach(function (v) { v.pause(); });
  });

  // ---- the download: the visitor's system first
  var form = document.querySelector(".installer");
  if (form) {
    var go = form.querySelector(".installer-go");
    var note = form.querySelector(".installer-note");
    var base = form.getAttribute("action").replace(/[^/]+$/, "");
    function pick(input) {
      go.href = base + input.getAttribute("data-file");
      go.textContent = "Download for " + input.nextElementSibling.textContent;
      note.textContent = input.getAttribute("data-note");
    }
    form.addEventListener("change", function (e) { if (e.target.name === "os") pick(e.target); });
    form.addEventListener("submit", function (e) { e.preventDefault(); window.location = go.href; });
    var ua = navigator.userAgent;
    // a phone or a tablet (an iPad says "Macintosh" but has touch points): no system of its own to offer
    var handheld = /iPhone|iPad|iPod|Android/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
    if (handheld) {
      form.querySelectorAll('input[name="os"]').forEach(function (i) { i.checked = false; });
      go.href = base.replace("/releases/download/", "/releases/tag/").replace(/\/$/, "");
      go.textContent = "All downloads";
      note.textContent = "Dussek runs on a computer (Windows, macOS or Linux), not on a phone or a tablet: choose the computer's system.";
    } else {
      var os = /Mac/.test(ua) ? "macos" : /Linux|X11/.test(ua) ? "linux" : "windows";
      var input = form.querySelector('input[value="' + os + '"]');
      if (input) { input.checked = true; pick(input); }
    }
  }

  // ---- entrances: each part of a section rising in, once
  if (!reduce && "IntersectionObserver" in window) {
    var parts = document.querySelectorAll(".field:not(.download) h2, .field .names, .field .work-note, .feature, .ensemble figure, .showcase-title");
    parts.forEach(function (el, i) {
      el.classList.add("rise");
      el.style.transitionDelay = (i % 4) * 75 + "ms";
    });
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting) { en.target.classList.add("in"); io.unobserve(en.target); }
      });
    }, { rootMargin: "0px 0px -8% 0px" });
    parts.forEach(function (el) { io.observe(el); });
    // never left hidden: whatever is above the screen's bottom edge, and everything once the page's end is reached
    function catchUp() {
      var bottom = window.innerHeight;
      var end = window.scrollY + bottom >= document.documentElement.scrollHeight - 4;
      parts.forEach(function (el) {
        if (!el.classList.contains("in") && (end || el.getBoundingClientRect().top < bottom)) { el.classList.add("in"); io.unobserve(el); }
      });
    }
    window.addEventListener("scroll", catchUp, { passive: true });
    window.addEventListener("load", catchUp);
    window.addEventListener("resize", catchUp);
  }
})();
