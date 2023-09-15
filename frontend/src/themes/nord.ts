import { theme } from './base';

const nord = {
  0: '#2e3440',
  1: '#3b4252',
  2: '#434c5e',
  3: '#4c566a',
  4: '#d8dee9',
  5: '#e5e9f0',
  6: '#eceff4',
  7: '#8fbcbb',
  8: '#88c0d0',
  9: '#81a1c1',
  10: '#5e81ac',
  11: '#bf616a',
  12: '#d08770',
  13: '#ebcb8b',
  14: '#a3be8c',
  15: '#b48ead',
};

export const nordTheme = theme({
  name: 'nord',
  scheme: 'dark',
  colors: {
    primary: nord[10],
    primaryFocus: nord[9],
    secondary: nord[7],
    accent: nord[15],
    neutral: nord[4],
    neutralFocus: nord[5],
    neutralContent: nord[0],
    base100: nord[0],
    base200: nord[1],
    base300: nord[2],
    base400: nord[3],
    info: nord[8],
    success: nord[14],
    warning: nord[13],
    error: nord[11],
    link: nord[8],
    linkVisited: nord[15],
  },
});

export const nordLightTheme = theme({
  name: 'nord-light',
  scheme: 'light',
  colors: {
    primary: nord[10],
    primaryFocus: nord[9],
    secondary: nord[8],
    accent: nord[15],
    neutral: nord[2],
    neutralFocus: nord[1],
    neutralContent: nord[4],
    base100: 'rgb(255, 255, 255)',
    base200: nord[6],
    base300: nord[5],
    baseContent: 'rgb(0, 0, 0)',
    info: nord[8],
    success: nord[14],
    warning: nord[13],
    error: nord[11],
    link: nord[10],
    linkVisited: nord[15],
  },
});
