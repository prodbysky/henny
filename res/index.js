queryForm.addEventListener("submit", async function (event) {
    event.preventDefault();
    const query = document.getElementById("query").value;
    const result = await fetch("http://127.0.0.1:6969/query?query=" + encodeURIComponent(query));
    const data = await result.json();
    const container = document.getElementById("results");
    container.innerHTML = "";

    data.forEach((item, i) => {
        const div = document.createElement("div");
        div.className = "result-item";
        div.textContent = item;

        div.style.animationDelay = (i * 0.02) + "s";

        container.appendChild(div);
    });
});
