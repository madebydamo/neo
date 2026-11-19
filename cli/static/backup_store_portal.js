// Rescue portal of the backup store (templates/backup_store/client.html.hbs):
// a chosen key file is read into the key field, so it is sent like a pasted key.
// The file never leaves the page any other way.
(function (root) {
  'use strict';
  if (!root || !root.document) return;
  root.document.addEventListener('change', function (e) {
    var input = e.target;
    if (!input || !input.matches || !input.matches('[data-neo-key-file]')) return;
    var file = input.files && input.files[0];
    var form = input.closest('form');
    var text = form && form.querySelector('[data-neo-key-text]');
    if (!file || !text) return;
    var reader = new root.FileReader();
    reader.onload = function () {
      text.value = String(reader.result || '');
      input.value = '';
    };
    reader.readAsText(file);
  });
})(typeof window !== 'undefined' ? window : null);
