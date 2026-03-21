const form = document.getElementById("queryForm");
const resultsEl = document.getElementById("results");
const resultsHeader = document.getElementById("results-header");
const resultCount = document.getElementById("result-count");

form.addEventListener("submit", async function (event) {
    event.preventDefault();
    const query = document.getElementById("query").value.trim();
    if (!query) return;

    const backend_url = `http://${window.location.hostname}:6969`;

    clearResults();
    showLoading();

    try {
        const result = await fetch(`${backend_url}/query?query=${encodeURIComponent(query)}&n_result=255`);
        const data = await result.json();

        clearResults();

        if (data.error) {
            showError(data.error);
            return;
        }

        if (data.results.length === 0) {
            showMessage("no results found");
            resultsHeader.classList.remove("visible");
            return;
        }

        resultCount.textContent = data.results.length;
        resultsHeader.classList.add("visible");

        data.results.forEach((item, i) => {
            const div = document.createElement("div");
            div.className = "result-item";
            div.style.animationDelay = `${i * 0.03}s`;

            const rank = document.createElement("span");
            rank.className = "result-rank";
            rank.textContent = String(i + 1).padStart(2, "0");

            const pathSpan = document.createElement("span");
            pathSpan.className = "result-path";
            pathSpan.appendChild(formatPath(item));

            const ext = item.split(".").pop().toLowerCase();
            const extBadge = document.createElement("span");
            extBadge.className = "result-ext";
            extBadge.textContent = ext;

            const arrow = document.createElement("span");
            arrow.className = "result-arrow";
            arrow.textContent = "Download";

            div.appendChild(rank);
            div.appendChild(pathSpan);
            div.appendChild(extBadge);
            div.appendChild(arrow);

            div.addEventListener("click", () => downloadFile(item, backend_url));
            div.setAttribute("title", item);

            resultsEl.appendChild(div);
        });
    } catch (e) {
        clearResults();
        showError("failed to reach backend at " + backend_url);
    }
});

function formatPath(fullPath) {
    const parts = fullPath.replace(/\\/g, "/").split("/");
    const filename = parts.pop();
    const dir = parts.join("/");

    const frag = document.createDocumentFragment();
    if (dir) {
        const dirSpan = document.createElement("span");
        dirSpan.className = "path-dir";
        dirSpan.textContent = dir + "/";
        frag.appendChild(dirSpan);
    }
    const fileSpan = document.createElement("span");
    fileSpan.className = "path-file";
    fileSpan.textContent = filename;
    frag.appendChild(fileSpan);
    return frag;
}

async function downloadFile(path, backend_url) {
    try {
        const response = await fetch(`${backend_url}/file?path=${encodeURIComponent(path)}`);
        if (!response.ok) {
            const data = await response.json();
            showError(data.error || "failed to download file");
            return;
        }

        const blob = await response.blob();
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");

        const disposition = response.headers.get("Content-Disposition") || "";
        const match = disposition.match(/filename="([^"]+)"/);
        a.download = match ? match[1] : path.split("/").pop();

        a.href = url;
        a.click();
        URL.revokeObjectURL(url);
    } catch (e) {
        showError("failed to download file");
    }
}

function clearResults() {
    resultsEl.innerHTML = "";
    resultsHeader.classList.remove("visible");
}

function showLoading() {
    const div = document.createElement("div");
    div.className = "result-item loading-item";

    const text = document.createElement("span");
    text.textContent = "querying index...";

    div.appendChild(text);
    resultsEl.appendChild(div);
}

function showMessage(text) {
    const div = document.createElement("div");
    div.className = "result-item message-item";
    div.textContent = text;
    resultsEl.appendChild(div);
}

function showError(text) {
    const div = document.createElement("div");
    div.className = "result-item error-item";
    div.textContent = text;
    resultsEl.appendChild(div);
}
