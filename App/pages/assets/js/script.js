// OpenPeripheral — Page Script
// Clock, toast notifications, and theme toggle

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

// ── Toast Notifications ──
function showToast(message, type) {
  type = type || 'info';
  var container = document.getElementById('toast-container');
  if (!container) return;
  var toast = document.createElement('div');
  toast.className = 'toast toast-' + type;
  toast.textContent = message;
  container.appendChild(toast);
  setTimeout(function() {
    if (toast.parentNode) {
      toast.parentNode.removeChild(toast);
    }
  }, 4000);
}
