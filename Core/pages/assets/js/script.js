// OpenPeripheral — Page Script
// Clock, toast notifications, and theme toggle

// ── Live Clock ──
var clockH = document.getElementById('clock-h');
var clockM = document.getElementById('clock-m');
var clockS = document.getElementById('clock-s');
if (clockH && clockM && clockS) {
  function updateClock() {
    var now = new Date();
    clockH.textContent = String(now.getHours()).padStart(2, '0');
    clockM.textContent = String(now.getMinutes()).padStart(2, '0');
    clockS.textContent = String(now.getSeconds()).padStart(2, '0');
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
    toast.classList.add('toast-exit');
    setTimeout(function() {
      if (toast.parentNode) toast.parentNode.removeChild(toast);
    }, 300);
  }, 4000);
}

// Progress toast: creates a toast with a progress bar.
// Returns a toast id that can be used with updateProgress / completeProgress.
var _toastIdCounter = 0;
function showProgressToast(message, type) {
  type = type || 'info';
  var container = document.getElementById('toast-container');
  if (!container) return null;
  var id = '_pt' + (++_toastIdCounter);
  var toast = document.createElement('div');
  toast.className = 'toast toast-progress toast-' + type;
  toast.id = id;
  var msg = document.createElement('span');
  msg.className = 'toast-message';
  msg.textContent = message;
  toast.appendChild(msg);
  var bar = document.createElement('div');
  bar.className = 'toast-progress-bar';
  var fill = document.createElement('div');
  fill.className = 'toast-progress-fill';
  fill.style.width = '0%';
  bar.appendChild(fill);
  toast.appendChild(bar);
  container.appendChild(toast);
  return id;
}

function updateProgress(id, percent, message) {
  var toast = document.getElementById(id);
  if (!toast) return;
  var fill = toast.querySelector('.toast-progress-fill');
  if (fill) fill.style.width = Math.min(100, Math.max(0, percent)) + '%';
  if (message) {
    var msg = toast.querySelector('.toast-message');
    if (msg) msg.textContent = message;
  }
}

function completeProgress(id, message, type) {
  var toast = document.getElementById(id);
  if (!toast) return;
  if (type) {
    toast.className = toast.className.replace(/toast-(info|warning)/, 'toast-' + type);
  }
  var fill = toast.querySelector('.toast-progress-fill');
  if (fill) fill.style.width = '100%';
  if (message) {
    var msg = toast.querySelector('.toast-message');
    if (msg) msg.textContent = message;
  }
  setTimeout(function() {
    toast.classList.add('toast-exit');
    setTimeout(function() {
      if (toast.parentNode) toast.parentNode.removeChild(toast);
    }, 300);
  }, 3000);
}
