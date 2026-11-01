module.exports = {
  onTitleOpened(t) {
    return { toast: 'Opened: ' + t.name };
  },
  async onClipSaved(c) {
    return { toast: 'Clip saved (' + (c.text || '').length + ' letters)' };
  },
};
