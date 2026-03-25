// OpenPeripheral — Preview Script
// Handles sidebar collapse toggle and live clock

document.addEventListener('DOMContentLoaded', () => {
  // ── Sidebar Collapse ──
  const sidebar = document.querySelector('.sidebar');
  const collapseBtn = document.querySelector('.collapse-btn');
  if (collapseBtn && sidebar) {
    collapseBtn.addEventListener('click', () => {
      sidebar.classList.toggle('collapsed');
    });
  }

  // ── Live Clock ──
  const clockEl = document.getElementById('clock');
  if (clockEl) {
    function updateClock() {
      const now = new Date();
      const h = String(now.getHours()).padStart(2, '0');
      const m = String(now.getMinutes()).padStart(2, '0');
      const s = String(now.getSeconds()).padStart(2, '0');
      clockEl.textContent = `${h}:${m}:${s}`;
    }
    updateClock();
    setInterval(updateClock, 1000);
  }

  // ── Theme Toggle ──
  const themeBtn = document.querySelector('[data-cmd="toggle-theme"]');
  if (themeBtn) {
    themeBtn.addEventListener('click', () => {
      document.body.classList.toggle('light-theme');
      themeBtn.classList.toggle('active');
    });
  }
});
