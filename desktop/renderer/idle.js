'use strict';
/* While the window is hidden or minimised, the page's polling timers do nothing: no requests, no redraws, no CPU. They carry on
   by themselves when the window is shown again (and the page refreshes once, so nothing looks old). Loaded before every other script. */
(() => {
  const native = window.setInterval.bind(window);
  window.setInterval = (fn, ms, ...a) => typeof fn !== 'function' ? native(fn, ms, ...a) : native((...x) => { if (!document.hidden) fn(...x); }, ms, ...a);
  document.addEventListener('visibilitychange', () => { if (!document.hidden) window.dispatchEvent(new Event('hearth-shown')); });
})();
