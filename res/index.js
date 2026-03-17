queryForm.addEventListener("submit", async function (event) {
    event.preventDefault();
    const query = document.getElementById("query").value;
    const backend_url = `http://${window.location.hostname}:6969`
    const container = document.getElementById("results");
    container.innerHTML = "";

    try {
        const result = await fetch(`${backend_url}/query?query=${encodeURIComponent(query)}`);
        const data = await result.json();

        if (data.error) {
            showError(data.error);
            return;
        }

        if (data.results.length === 0) {
            showMessage("No results found");
            return;
        }

        data.results.forEach((item, i) => {
            const div = document.createElement("div");
            div.className = "result-item";
            div.textContent = item;
            div.style.animationDelay = (i * 0.02) + "s";
            div.style.cursor = "pointer";

            div.addEventListener("click", () => downloadFile(item, backend_url));

            container.appendChild(div);
        });
    } catch (e) {
        showError("Failed to fetch the backend");
        return;
    }
});

async function downloadFile(path, backend_url) {
    try {
        const response = await fetch(`${backend_url}/file?path=${encodeURIComponent(path)}`);
        if (!response.ok) {
            const data = await response.json();
            showError(data.error || "Failed to download file");
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
        showError("Failed to download file");
    }
}

function showMessage(text) {
    const div = document.createElement("div");
    div.className = "result-item";
    div.textContent = text;
    document.getElementById("results").appendChild(div);
}

function showError(text) {
    const div = document.createElement("div");
    div.className = "result-item error-item";
    div.textContent = "Error: " + text;
    document.getElementById("results").appendChild(div);
}
