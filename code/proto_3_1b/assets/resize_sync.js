/* ヘッダーラベル列のリサイズを全行に同期（CSS カスタムプロパティ方式） */
(function () {
    var resizeObs = null;
    var currentHeader = null;

    function sync() {
        var header = document.getElementById("wf-header-label");
        if (header === currentHeader) return;

        if (resizeObs) resizeObs.disconnect();
        currentHeader = header;
        if (!header) return;

        resizeObs = new ResizeObserver(function () {
            var container = document.getElementById("waveform-container");
            if (container) {
                container.style.setProperty(
                    "--label-col-width", header.offsetWidth + "px"
                );
            }
        });
        resizeObs.observe(header);
    }

    /* waveform-container の子要素が入れ替わるたびに再設定 */
    var mo = new MutationObserver(sync);
    var container = document.getElementById("waveform-container");
    if (container) {
        mo.observe(container, { childList: true });
        sync();
    } else {
        /* Dash のレイアウト描画を待つ */
        var boot = new MutationObserver(function () {
            var c = document.getElementById("waveform-container");
            if (c) {
                boot.disconnect();
                mo.observe(c, { childList: true });
                sync();
            }
        });
        boot.observe(document.body, { childList: true, subtree: true });
    }
})();
