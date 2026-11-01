// Dussek's website: one player for the page (a play button, a piece's name, a feature's step), the
// features' A/B (a step switched while playing keeps its place), the demos' cards travelling across
// the held picture, the download's system, the entrances. Without it the page still works: the
// buttons are then inert and the video and the download links are plain.
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
  bar.innerHTML = '<span class="now-title"></span><span class="now-time">0:00</span><button type="button">Pause</button>';
  document.body.appendChild(bar);
  var barTitle = bar.querySelector(".now-title");
  var barTime = bar.querySelector(".now-time");
  var barButton = bar.querySelector("button");

  function mark(control, on) {
    if (control) control.setAttribute("aria-pressed", on ? "true" : "false");
  }
  function clock(t) {
    t = Math.max(0, Math.floor(t || 0));
    return Math.floor(t / 60) + ":" + String(t % 60).padStart(2, "0");
  }
  function play(control, keepTime) {
    var src = control.getAttribute("data-src");
    var at = keepTime ? audio.currentTime : 0;
    if (current === control && !audio.paused) {
      audio.pause();
      return;
    }
    if (current && current !== control) mark(current, false);
    current = control;
    if (audio.getAttribute("src") !== src) {
      audio.src = src;
      if (at > 0) {
        audio.addEventListener("loadedmetadata", function seek() {
          audio.removeEventListener("loadedmetadata", seek);
          audio.currentTime = Math.min(at, Math.max(0, audio.duration - 0.05));
        });
      }
    }
    barTitle.textContent = control.getAttribute("data-title");
    bar.classList.add("on");
    audio.play().catch(function () {});
  }
  audio.addEventListener("play", function () { mark(current, true); barButton.textContent = "Pause"; });
  audio.addEventListener("pause", function () { mark(current, false); barButton.textContent = "Play"; });
  audio.addEventListener("ended", function () { mark(current, false); barButton.textContent = "Play"; });
  audio.addEventListener("timeupdate", function () { barTime.textContent = clock(audio.currentTime) + " / " + clock(audio.duration); });
  barButton.addEventListener("click", function () {
    if (audio.paused) audio.play().catch(function () {}); else audio.pause();
  });

  document.addEventListener("click", function (e) {
    var b = e.target.closest("[data-src]");
    if (!b) return;
    e.preventDefault();
    // a feature's steps: the same phrase, so switching keeps the time
    var same = b.classList.contains("step") && current && current.classList.contains("step") &&
      current.closest(".feature") === b.closest(".feature") && current !== b && !audio.paused;
    play(b, same);
  });
  document.querySelectorAll("[data-src]").forEach(function (b) { b.setAttribute("aria-pressed", "false"); });
  // the video and the page's audio: one at a time
  document.querySelectorAll("video").forEach(function (v) {
    v.addEventListener("play", function () { audio.pause(); });
  });
  audio.addEventListener("play", function () {
    document.querySelectorAll("video").forEach(function (v) { v.pause(); });
  });

  // ---- the demos' cards across the held picture (desktop, motion allowed)
  var showcase = document.querySelector(".showcase");
  var track = showcase && showcase.querySelector(".cards");
  var wide = window.matchMedia("(min-width: 761px)");
  function travel() {
    if (!track) return;
    if (reduce || !wide.matches) { track.style.removeProperty("--shift"); return; }
    // two whole cards in view at a time, stepping one card on as the page scrolls
    var r = showcase.getBoundingClientRect();
    var run = showcase.offsetHeight - window.innerHeight;
    var p = Math.min(1, Math.max(0, -r.top / Math.max(1, run)));
    var cards = track.children;
    if (cards.length < 3) return;
    var step = cards[1].offsetLeft - cards[0].offsetLeft;
    var index = Math.round(p * (cards.length - 2));
    track.style.setProperty("--shift", (-index * step) + "px");
  }
  window.addEventListener("scroll", travel, { passive: true });
  window.addEventListener("resize", travel);
  travel();

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
    var os = /Mac/.test(ua) ? "macos" : /Linux|X11/.test(ua) && !/Android/.test(ua) ? "linux" : "windows";
    var input = form.querySelector('input[value="' + os + '"]');
    if (input) { input.checked = true; pick(input); }
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
