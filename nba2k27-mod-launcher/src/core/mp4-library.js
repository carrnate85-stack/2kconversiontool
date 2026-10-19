'use strict';
const fs = require('node:fs/promises');
const {constants} = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {inspectMp4} = require('./mp4-metadata');
const ID = /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/;

// Separate manifest: never reinterpret, overwrite or delete a user's native MOV library.
class Mp4Library {
  constructor({libraryDirectory, isGameRunning = async () => false, isPlaying = () => false, onProgress = () => {}, inspect = inspectMp4}) {
    this.directory = path.resolve(libraryDirectory);
    Object.assign(this, {isGameRunning, isPlaying, onProgress, inspect});
    this.busy = false;
  }
  file(id) { if (!ID.test(id)) throw Error('Invalid video ID.'); return path.join(this.directory, 'mp4', id + '.mp4'); }
  async checkDirectory() {
    for (const folder of [this.directory, path.join(this.directory, 'mp4')]) {
      const stat = await fs.lstat(folder).catch(e => { if (e.code === 'ENOENT') return null; throw e; });
      if (stat && (!stat.isDirectory() || stat.isSymbolicLink())) throw Error('The video library must use regular folders, not links.');
    }
  }
  async library() {
    await this.checkDirectory();
    let value;
    try { value = JSON.parse(await fs.readFile(path.join(this.directory, 'mp4-library.json'), 'utf8')); }
    catch (e) { if (e.code === 'ENOENT') return {schema: 1, selected: null, items: []}; throw Error('The MP4 library could not be read. Restore mp4-library.json before editing it.'); }
    if (value.schema !== 1 || !Array.isArray(value.items) || value.items.some(i => !ID.test(i.id) || typeof i.name !== 'string' || !i.name.trim() || i.name.length > 120) || new Set(value.items.map(i => i.id)).size !== value.items.length || (value.selected !== null && !value.items.some(i => i.id === value.selected))) throw Error('Invalid MP4 library.');
    return value;
  }
  async saveLibrary(value) {
    await this.checkDirectory();
    await fs.mkdir(this.directory, {recursive: true});
    const target = path.join(this.directory, 'mp4-library.json'), temp = target + '.' + crypto.randomUUID() + '.tmp';
    try { await fs.writeFile(temp, JSON.stringify(value, null, 2), {flag: 'wx'}); await fs.rename(temp, target); }
    finally { await fs.rm(temp, {force: true}); }
  }
  async assertEditable() {
    if (this.isPlaying()) throw Error('Close the video preview before changing the library.');
    if (await this.isGameRunning()) throw Error('Close NBA 2K27 before changing startup videos.');
  }
  cancel() { if (this.controller) this.controller.abort(); }
  async view() {
    const library = await this.library();
    const movie = {present: false, path: null};
    if (library.selected) {
      movie.path = this.file(library.selected);
      try { movie.info = await this.inspect(movie.path); movie.present = true; movie.bytes = movie.info.bytes; }
      catch (e) { movie.error = e.code === 'ENOENT' ? 'The selected MP4 is missing. Add it again or choose another video.' : e.message; }
    }
    const legacyPresent = Boolean(await fs.lstat(path.join(this.directory, 'startup.mov')).catch(() => null));
    return {directory: this.directory, library, movie, legacyPresent};
  }
  async importMovie(_root, source) {
    if (this.busy) throw Error('A video operation is already in progress.');
    this.busy = true; this.controller = new AbortController();
    let target, committed = false;
    try {
      await this.assertEditable();
      this.onProgress({percent: 0, message: 'Checking MP4…'});
      const info = await this.inspect(source), data = await this.library(), id = crypto.randomUUID();
      target = this.file(id);
      await fs.mkdir(path.dirname(target), {recursive: true});
      this.onProgress({percent: 20, message: 'Copying MP4 into Library / Videos…'});
      await fs.copyFile(source, target, constants.COPYFILE_EXCL);
      await this.inspect(target);
      await this.assertEditable();
      if (this.controller.signal.aborted) throw Error('Video import was cancelled.');
      data.items.push({id, name: path.basename(source, path.extname(source)).slice(0, 120), ...info});
      if (!data.selected) data.selected = id;
      await this.saveLibrary(data); committed = true;
      this.onProgress({percent: 100, message: 'MP4 added. No conversion needed.'});
      return await this.view();
    } finally {
      try { if (target && !committed) await fs.rm(target, {force: true}); }
      finally { this.busy = false; this.controller = null; }
    }
  }
  async editLibrary(_root, action, id, name) {
    if (this.busy) throw Error('Wait for the video operation to finish.');
    this.busy = true;
    try {
      await this.assertEditable();
      const data = await this.library(), item = data.items.find(i => i.id === id);
      if (!item) throw Error('Video not found. Refresh the library.');
      if (action === 'rename') {
        if (typeof name !== 'string' || !name.trim() || name.trim().length > 120) throw Error('Enter a video name between 1 and 120 characters.');
        item.name = name.trim();
      } else if (action === 'select') { await this.inspect(this.file(id)); data.selected = id; }
      else if (action === 'remove') {
        data.items = data.items.filter(i => i.id !== id);
        if (data.selected === id) data.selected = null;
      } else throw Error('Invalid video operation.');
      await this.assertEditable();
      await this.saveLibrary(data);
      if (action === 'remove') await fs.rm(this.file(id), {force: true});
      return await this.view();
    } finally { this.busy = false; }
  }
  async previewFile(id) {
    if (this.busy) throw Error('Wait for the video operation to finish.');
    await this.assertEditable();
    const data = await this.library(), selected = id || data.selected;
    if (!data.items.some(i => i.id === selected)) throw Error('Choose an MP4 first.');
    const file = this.file(selected); await this.inspect(file); return file;
  }
}
module.exports = {Mp4Library};
