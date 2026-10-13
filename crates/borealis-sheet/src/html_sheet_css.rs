//! Stylesheet of the HTML sheet preview. Tokens differ for colour and ink; everything else is shared.

/// Colour tokens (light paper).
const COLOUR: &str = "--paper:#FBF9F4;--ink:#1F272B;--ink-2:#4E585D;--ink-3:#6F736F;--rule:#CFC9BC;--field:#F3EFE6;--gold:#B38000;--wash:#305754;";
/// Ink tokens: greys only.
const INK: &str = "--paper:#FFFFFF;--ink:#000000;--ink-2:#404040;--ink-3:#6B6B6B;--rule:#BFBFBF;--field:#F0F0F0;--gold:#555555;--wash:#404040;";

/// The `:root` token block.
pub(super) fn tokens(ink: bool) -> String {
    format!(":root{{{}}}", if ink { INK } else { COLOUR })
}

/// Shared rules.
pub(super) const CSS: &str = r#"
*{box-sizing:border-box}
html{-webkit-print-color-adjust:exact;print-color-adjust:exact}
body{margin:0;background:#8f8f8f;font:7.4pt/1.32 "Noto Sans","Segoe UI",system-ui,-apple-system,sans-serif;color:var(--ink)}
body.compact{font-size:6.6pt;line-height:1.25}
.serif,.name,h2,.ab-name,.mod,.big,.mid,.lvhead td{font-family:"Noto Serif",Georgia,"Times New Roman",serif}
.desk{padding:16px 0}
.page{position:relative;display:flex;background:var(--paper);margin:0 auto 16px;box-shadow:0 2px 10px rgba(0,0,0,.35);overflow:hidden}
.letter{width:612pt;min-height:792pt}.a4{width:595pt;min-height:842pt}
.frame,.corner,.sky{position:absolute;pointer-events:none;color:var(--ink);overflow:hidden}
.frame{inset:0;width:100%;height:100%}
.corner{width:62pt;height:62pt}
.c-tl{left:13pt;top:13pt}.c-tr{right:13pt;top:13pt;transform:scaleX(-1)}
.c-bl{left:13pt;bottom:13pt;transform:scaleY(-1)}.c-br{right:13pt;bottom:13pt;transform:scale(-1,-1)}
.sky{left:-16pt;top:-14pt;height:78pt;width:calc(100% + 32pt);opacity:.4}
.content{position:relative;flex:1;min-width:0;padding:40pt 42pt 50pt;display:flex;flex-direction:column;gap:7pt}
.foot{position:absolute;left:70pt;right:70pt;bottom:30pt;display:flex;justify-content:space-between;font-size:5.6pt;color:var(--ink-3)}
.attr{font-size:5.2pt;color:var(--ink-3);line-height:1.3;margin-top:auto}
.head{display:grid;grid-template-columns:1fr auto;gap:4pt 14pt;align-items:end;position:relative}
.name{font-weight:700;font-size:23pt;line-height:1.05}
.ident{font-size:8.4pt;color:var(--ink-2);margin-top:2pt}.ident b{color:var(--ink)}
.meta{display:flex;gap:2pt 10pt;text-align:right}
.meta .lbl{display:block}.meta .val{font-weight:700;font-size:8pt}
.divider{height:20pt;width:100%;color:var(--ink);margin:-4pt 0}
.lbl{font-size:5.4pt;font-weight:700;text-transform:uppercase;letter-spacing:.09em;color:var(--ink-2)}
[dir=auto]{unicode-bidi:plaintext}
.fv:empty.hide,.opt:empty{display:none}
.paren:not(:empty)::before{content:"("}.paren:not(:empty)::after{content:")"}
.ms:not(:empty)::before{content:"mastery "}
.f{display:block;background:var(--field);border-radius:2pt;min-height:13pt;padding:1.5pt 4pt}
.calc{font-weight:700}
.blank{background:var(--field);border:.6pt dashed var(--rule)}
h2{margin:0 0 3pt;font-weight:700;font-size:8.6pt;line-height:1.2;letter-spacing:.03em;display:flex;align-items:center;gap:5pt}
h2 svg.hr{flex:1;min-width:0;width:0;height:12pt;color:var(--ink-3);overflow:hidden}
h2 .aside{font:400 6pt "Noto Sans",sans-serif;color:var(--ink-3);letter-spacing:0}
.sec{min-height:0}.grow{flex:1;display:flex;flex-direction:column}
.body{display:grid;grid-template-columns:178pt minmax(0,1fr);gap:10pt;flex:1}
.a4 .body.d14{grid-template-columns:150pt minmax(0,1fr)}
.abil,.main,.col{display:flex;flex-direction:column;gap:4.5pt;min-width:0}
.main,.col{gap:7pt}
.ab{border:.7pt solid var(--rule);border-radius:3pt;padding:4pt 5pt 3pt;background:var(--field)}
.ab-top{display:grid;grid-template-columns:27pt 1fr auto auto;align-items:center;gap:5pt}
.ab-top svg{width:27pt;height:27pt;color:var(--ink)}
.ab-name{font-weight:700;font-size:8.2pt;letter-spacing:.04em}
.ab-name small{display:block;font:400 5.4pt "Noto Sans",sans-serif;color:var(--ink-3);letter-spacing:.08em;text-transform:uppercase}
.mod{font-weight:700;font-size:17pt;line-height:1;min-width:30pt;text-align:center}
.score{width:20pt;height:20pt;border:.8pt solid var(--gold);border-radius:50%;display:grid;place-items:center;font-weight:700;font-size:8pt}
.rows{margin-top:2pt;border-top:.5pt solid var(--rule);padding-top:1.5pt}
.r{display:grid;grid-template-columns:9pt 1fr auto;align-items:center;gap:2pt;line-height:1.38}
.r.s14{grid-template-columns:9pt 1fr 18pt auto}
.r.save{font-weight:700}.r .v{font-weight:700;min-width:18pt;text-align:right}
.src{color:var(--ink-3);font-size:5.8pt}
.pip{width:6pt;height:6pt;border:.7pt solid var(--ink-2);border-radius:50%;display:inline-block}
.pip.on{background:var(--ink);border-color:var(--ink)}
.pip.ex{background:var(--gold);border-color:var(--gold);transform:rotate(45deg);border-radius:1pt;width:5.5pt;height:5.5pt}
.mk{display:inline-flex}.mk.ex .p,.mk:not(.ex) .e{display:none}
.pip.na{visibility:hidden}
.vitals{display:grid;grid-template-columns:62pt minmax(0,1fr) 92pt;gap:6pt}
.box{border:.7pt solid var(--rule);border-radius:3pt;padding:3pt 5pt;position:relative}
.ac{text-align:center;padding-top:5pt;border-color:var(--ink-2);border-radius:3pt 3pt 50% 50%/3pt 3pt 22% 22%}
.big{font-weight:700;font-size:20pt;line-height:1.05;display:block}
.note{font-size:5.8pt;color:var(--ink-2)}
.hp{display:grid;grid-template-columns:auto 1fr 1fr;gap:3pt 6pt;align-items:end}.hp .f{height:24pt}
.trk{display:flex;gap:2.5pt;align-items:center;flex-wrap:wrap}
.dot{width:7pt;height:7pt;border:.7pt solid var(--ink-2);border-radius:50%;display:inline-block}
.dot.g{border-color:var(--gold)}.dot.x{border-radius:1pt}
.strip{display:grid;grid-template-columns:repeat(var(--n,6),1fr);gap:5pt}
.strip .box{text-align:center;padding:3pt 2pt}
.mid{font-weight:700;font-size:11.5pt;line-height:1.15;display:block}
.insp{width:11pt;height:11pt;margin:2pt auto 0;border:.9pt solid var(--gold);transform:rotate(45deg);display:block}
table{width:100%;border-collapse:collapse}
th{text-align:left;font-size:5.3pt;font-weight:700;text-transform:uppercase;letter-spacing:.08em;color:var(--ink-2);padding:1pt 3pt;border-bottom:.7pt solid var(--ink-2)}
td{padding:1.6pt 3pt;border-bottom:.5pt solid var(--rule);vertical-align:top}
td.n{text-align:right;font-weight:700;white-space:nowrap}th.n{text-align:right;white-space:nowrap}
td.c{text-align:center}th.c{text-align:center}
tr.empty td{height:12pt}
.kv{display:grid;grid-template-columns:58pt 1fr;gap:1.5pt 6pt}
.kv>div:nth-child(odd){font-weight:700;color:var(--ink-2);font-size:6.4pt;padding-top:.5pt}
.chips{display:flex;flex-wrap:wrap;gap:2pt 7pt}.chip{display:inline-flex;gap:2.5pt;align-items:center}
.sq{width:6.5pt;height:6.5pt;border:.7pt solid var(--ink-2);border-radius:1pt;display:inline-block}
.sq.on{background:var(--ink)}
.two{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:12pt;flex:1}
.sub{display:grid;grid-template-columns:150pt minmax(0,1fr);gap:10pt}
.feat{margin-bottom:4pt;break-inside:avoid}
.feat .t{font-weight:700}.feat .tag{font-size:5.6pt;color:var(--gold);font-weight:700;letter-spacing:.06em;text-transform:uppercase;margin-left:3pt}
.feat .txt{white-space:pre-line}
.feat.plain .t{font-weight:400}.feat.plain .tag{display:none}
.coins{display:grid;grid-template-columns:repeat(5,1fr);gap:4pt;text-align:center}.coins .f{font-weight:700;font-size:9pt}
.slots{display:grid;grid-template-columns:repeat(9,1fr);gap:4pt}
.slots .box{text-align:center;padding:2pt 1pt 3pt}.slots .off{opacity:.35}
.slots .lv{display:flex;justify-content:center;gap:2pt;font-weight:700;font-size:7pt;color:var(--wash)}
.cast{display:grid;grid-template-columns:1.4fr repeat(4,1fr);gap:5pt}.cast .box{text-align:center}.cast .box.l{text-align:left}
td.sp{font-weight:700}.book td.sp{font-weight:400;color:var(--ink-2)}
tr.lvhead td{background:var(--field);font-weight:700;font-size:6.3pt;letter-spacing:.05em;padding:1.4pt 3pt}
.lg{display:grid;grid-template-columns:repeat(3,1fr);gap:10pt;margin-top:6pt}.lg h3{display:flex;justify-content:space-between;font:700 8.4pt var(--serif,serif);border-bottom:.6pt solid var(--ink-2);margin:6pt 0 0;padding-bottom:1pt}.lg h3 span{font:italic 400 6.2pt var(--sans,sans-serif);color:var(--ink-2)}.lgr{display:flex;align-items:center;gap:3pt;height:14pt;border-bottom:.5pt solid var(--rule)}.lgr .fv{flex:1;min-height:9pt}.lgn{text-align:right;font-style:italic;font-size:6.6pt}
.lined{background:repeating-linear-gradient(to bottom,transparent 0 10.5pt,var(--rule) 10.5pt 11pt);line-height:11pt;white-space:pre-wrap;min-height:44pt;flex:1}
.lined.sm{min-height:24pt}
.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.tagline{font-size:5.6pt;color:var(--ink-3)}
.edition{font-size:5.6pt;font-weight:700;color:var(--wash);letter-spacing:.1em;text-transform:uppercase}
.story{display:grid;grid-template-columns:repeat(4,1fr);gap:4pt}
.story .f{min-height:16pt;font-weight:700}
.portrait{width:100%;max-height:170pt;object-fit:contain;border:.7pt solid var(--rule);border-radius:3pt;display:block}
.portrait.empty{height:120pt;display:grid;place-items:center;color:var(--ink-3);font-size:6pt}
.txtbox{white-space:pre-wrap;min-height:30pt}
.cards{flex:1;display:grid;grid-template-columns:repeat(3,minmax(0,1fr));grid-template-rows:repeat(3,minmax(0,1fr));gap:6pt;min-height:0}
.card{border:.8pt solid var(--rule);border-radius:4pt;padding:5pt;display:flex;flex-direction:column;overflow:hidden;min-height:0}
.card h3{margin:0;font:700 10pt/1.25 "Noto Serif",Georgia,serif}
.csub{font-size:6.2pt;font-style:italic;color:var(--ink-2);border-bottom:.6pt solid var(--rule);padding-bottom:2pt;margin-bottom:3pt}
.cfact{font-size:6.5pt}
.cbody{flex:1;min-height:0;overflow:hidden;font-size:6pt;line-height:1.25;white-space:pre-line}
.cfoot{display:flex;justify-content:space-between;gap:4pt;border-top:.6pt solid var(--rule);padding-top:2pt;font-size:5.8pt;font-style:italic;color:var(--ink-2)}
.cfoot .r{text-align:right}
@media(max-width:700px){.desk{zoom:.55}}
@media print{body{background:none}.desk{padding:0;zoom:1}.page{margin:0;box-shadow:none;break-after:page}.page:last-child{break-after:auto}
.letter{min-height:791pt}.a4{min-height:841pt}}
"#;
