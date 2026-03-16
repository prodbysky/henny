queryForm.addEventListener("submit", async function (event) {
    event.preventDefault();
    const query = document.getElementById("query").value;
    const backend_url = `http://${window.location.hostname}:6969`
    const result = await fetch(`${backend_url}/query?query=${encodeURIComponent(query)}`);
    const data = await result.json();
    const container = document.getElementById("results");
    container.innerHTML = "";

    if (data.length == 0) {
        const div = document.createElement("div");
        div.className = "result-item";
        div.textContent = "No results found";

        container.appendChild(div);
    }
    data.forEach((item, i) => {
        const div = document.createElement("div");
        div.className = "result-item";
        div.textContent = item;

        div.style.animationDelay = (i * 0.02) + "s";

        container.appendChild(div);
    });
});
