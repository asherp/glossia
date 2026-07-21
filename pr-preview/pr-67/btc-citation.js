// btc-citation.js — the book's three-tier block numbering. Volume = a
// halving era (210,000 blocks -- the block subsidy halves at each
// boundary). Book = a difficulty-adjustment window (2016 blocks -- Bitcoin
// retargets every 2016 blocks) within that era. Chapter = a block's
// position within its book.
//
// 210000 isn't a multiple of 2016 (210000/2016 ~= 104.17), so book
// numbering restarts at 1 with each volume rather than counting real global
// difficulty periods -- the last book of every era is a shorter, truncated
// one (336 blocks instead of 2016).
//
// Used by bitcoin-book.html to place each block within the volume/book/chapter
// scheme.

const ERA_BLOCKS = 210000;
const DIFFICULTY_BLOCKS = 2016;

// The inverse of volumeBookChapter: the block height at volume V, book B,
// chapter C (all 1-based). Chapter 1 is a book's first block, so a partial
// reference like "III 2" (era 3, book 2) resolves here with chapter defaulting
// to 1. Not clamped to a book's real length -- a chapter past the book's end
// spills into the following book, exactly as the forward formula implies.
export function heightOf(volume, book, chapter) {
  return (volume - 1) * ERA_BLOCKS + (book - 1) * DIFFICULTY_BLOCKS + (chapter - 1);
}

export function volumeBookChapter(height) {
  const volumeIndex = Math.floor(height / ERA_BLOCKS);
  const eraStart = volumeIndex * ERA_BLOCKS;
  const offsetInEra = height - eraStart;
  const bookIndex = Math.floor(offsetInEra / DIFFICULTY_BLOCKS);
  const bookStart = eraStart + bookIndex * DIFFICULTY_BLOCKS;
  const bookLength = Math.min(DIFFICULTY_BLOCKS, ERA_BLOCKS - bookIndex * DIFFICULTY_BLOCKS);
  return {
    volume: volumeIndex + 1,
    book: bookIndex + 1,
    chapter: height - bookStart + 1,
    chapterCount: bookLength,
  };
}

// A volume number as a Roman numeral (the book cites volumes in Roman).
export function toRoman(n) {
  const map = [[1000, 'M'], [900, 'CM'], [500, 'D'], [400, 'CD'], [100, 'C'], [90, 'XC'], [50, 'L'], [40, 'XL'], [10, 'X'], [9, 'IX'], [5, 'V'], [4, 'IV'], [1, 'I']];
  let out = '';
  for (const [v, s] of map) while (n >= v) { out += s; n -= v; }
  return out || '0';
}

// The scripture-style reference for a block height: Roman volume, then book and
// chapter (e.g. "III 2 5"). A transaction adds a §section to this.
export function reference(height) {
  const { volume, book, chapter } = volumeBookChapter(height);
  return `${toRoman(volume)} ${book} ${chapter}`;
}
