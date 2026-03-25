// OpenPeripheral — Page Script
// Clock, sidebar collapse, and theme toggle

// ── Live Clock ──
var clockEl = document.getElementById('clock');
if (clockEl) {
  function updateClock() {
    var now = new Date();
    var h = String(now.getHours()).padStart(2, '0');
    var m = String(now.getMinutes()).padStart(2, '0');
    var s = String(now.getSeconds()).padStart(2, '0');
    clockEl.textContent = h + ':' + m + ':' + s;
  }
  updateClock();
  setInterval(updateClock, 1000);
}
