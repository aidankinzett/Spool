/* Density tokens — Spool's two modes.
   These are the *only* numbers that change when Touch Mode flips on.
   Everything visual reads from these so we can present desktop and
   touch side-by-side in the canvas with no other code differences. */

window.TOUCH_DENSITY = {
  desktop: {
    label: "Desktop",
    pointer: "fine",
    /* type */
    base: 13, sm: 11, xs: 10.5,
    h1: 32, h2: 18, h3: 15,
    /* window chrome */
    titleBar: 40, titleBtnW: 46, titleBtnIcon: 12,
    /* buttons */
    btnH: 32, btnHsm: 26, primaryH: 46,
    iconBtn: 46, iconBtnSize: 18,
    /* lists & cards */
    rowH: 56, rowPadY: 8, rowGap: 12,
    cardPad: 18, sectionGap: 16,
    chipH: 24, chipPadX: 10,
    pageGutter: 32,
    /* sidebar */
    sidebarW: 320, thumbSm: 30, thumbXl: 96,
    /* stats */
    statsPadY: 18, statSize: 17,
    /* radios / candidate rows */
    radio: 16, radioDot: 7,
    confidenceBar: 64, chevron: 14,
    /* misc */
    scrollW: 10,
  },

  touch: {
    label: "Touch",
    pointer: "coarse",
    base: 15, sm: 13, xs: 12,
    h1: 40, h2: 22, h3: 17,
    titleBar: 56, titleBtnW: 64, titleBtnIcon: 16,
    btnH: 48, btnHsm: 40, primaryH: 64,
    iconBtn: 56, iconBtnSize: 22,
    rowH: 80, rowPadY: 14, rowGap: 18,
    cardPad: 22, sectionGap: 20,
    chipH: 40, chipPadX: 18,
    pageGutter: 28,
    sidebarW: 380, thumbSm: 48, thumbXl: 128,
    statsPadY: 22, statSize: 22,
    radio: 24, radioDot: 11,
    confidenceBar: 88, chevron: 20,
    scrollW: 18,
  },
};
