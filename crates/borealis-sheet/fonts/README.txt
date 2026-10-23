Noto Sans, Noto Serif, Noto Sans Hebrew and Noto Sans Arabic (Noto Project Authors), SIL Open Font
License 1.1. Sources: github.com/notofonts (unhinted/ttf). NotoSerif-Regular.ttf and NotoSerif-Italic.ttf
are the hinted notofonts builds (version 2.015, ttfautohint). NotoSansArabic-Regular.ttf and
NotoSansArabic-Bold.ttf are the unhinted static builds of release NotoSansArabic-v2.013. Subset at render
time; the files are not modified. See OFL-*.txt (OFL-NotoSerif.txt covers every Noto Serif style).

No CJK font is bundled: Han, kana and Hangul text is drawn with a font installed on the user's system
(src/os_fonts.rs), subset into the PDF the same way, and never shipped with Borealis.
