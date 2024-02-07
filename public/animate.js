window.onload = function () {
    const container = document.getElementById("container");
    const containerHeight = container.offsetHeight;
    const windowHeight = window.innerHeight;
    const creditsHeight = Math.ceil(containerHeight / windowHeight) * -100 - 10
    const animationDuration = ((containerHeight / windowHeight * 100) + 100) * 60;

    console.log(creditsHeight);

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
