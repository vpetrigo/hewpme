window.onload = function () {
    let container = document.getElementById("container");
    const containerHeight = container.offsetHeight;
    const windowHeight = window.innerHeight;
    const creditsHeight = Math.ceil(containerHeight / windowHeight) * -100
    const animationDuration = containerHeight / windowHeight * 20000;

    container.animate([
            {
                // from
                top: '105%',
            },
            {
                // to
                top: `${creditsHeight}%`,
            },
        ],
        {
            duration: animationDuration,
            iterations: Infinity
        });
}
