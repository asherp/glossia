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
