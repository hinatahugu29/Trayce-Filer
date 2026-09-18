/**
 * 「場所を中クリック＝隣のペインに開く」。
 *
 * 左クリックが「そこへ行く（いまのペインが動く）」なのに対し、中クリックは
 * 「いまのペインを動かさずに、隣へ出す」。場所が並ぶ面すべてで同じ意味にするため、
 * 判定をここ1か所に置く。
 *
 * ブラウザのタブ中クリックからの連想だが、行き先は窓ではなくペインである。
 * タブが担っているのは「安く作れて安く捨てられる入れ物」で、Trayce でそれに
 * あたるのはペインだから（窓は位置を持ち、閉じるのに `Alt+F4` が要る）。
 * 中ボタンはホイールそのもので指が滑るため、重い結果を置かない、という
 * `Alt+Q`／`Alt+W` と同じ線引きでもある。
 */

/** MouseEvent.button の中ボタン。 */
const MIDDLE = 1

/**
 * 中ボタンの押し下げを無害化する。
 *
 * Windows は中ボタン押し下げでオートスクロール（丸いカーソルが出るやつ）を
 * 始めるため、潰しておかないと「隣に開く」と同時に画面がスクロールし始める。
 * `auxclick` は離した時なので、こちらは `mousedown` に付ける。
 */
export function armMiddleClick(ev: MouseEvent): void {
  if (ev.button === MIDDLE) ev.preventDefault()
}

/**
 * 中ボタンで離された時だけ `run` を呼ぶ `auxclick` ハンドラを作る。
 *
 * `auxclick` は左以外のボタンで発火するので、右クリック（`contextmenu` が
 * 別に受け持つ）を巻き込まないようボタン番号で絞る。伝播も止める——場所の行は
 * 入れ子になっていることがあり（ツリー、その場展開した子）、止めないと祖先の行が
 * 同じ中クリックで自分の場所を開いてしまう。
 */
export function onMiddleClick(run: () => void): (ev: MouseEvent) => void {
  return (ev: MouseEvent) => {
    if (ev.button !== MIDDLE) return
    ev.preventDefault()
    ev.stopPropagation()
    run()
  }
}
