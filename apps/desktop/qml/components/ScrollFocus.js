.pragma library

function reveal(scroll, item) {
    const view = scroll.contentItem;
    if (!item.activeFocus || !view || !view.contentItem) return;
    const top = item.mapToItem(view.contentItem, 0, 0).y;
    const bottom = top + item.height;
    const margin = Math.min(36, view.height / 4);
    let target = view.contentY;
    if (top < target + margin) target = top - margin;
    else if (bottom > target + view.height - margin) target = bottom - view.height + margin;
    view.contentY = Math.max(0, Math.min(target, view.contentHeight - view.height));
}
