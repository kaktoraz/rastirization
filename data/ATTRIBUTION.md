# Test-image attribution

The benchmark uses six clean grayscale scenes in `data/clean/`.

| File | Origin / purpose | License or status |
|---|---|---|
| `baboon.png`, `board.png`, `fruits.png` | Existing project scenes converted from the bundled OpenCV sample sources in `data/sources/` | BSD-3-Clause sample material (as documented for the original project) |
| `astronaut.png` | Eileen Collins portrait from `skimage.data.astronaut`, resized only for the experiment | NASA/public-domain sample image distributed by scikit-image |
| `grace_hopper.png` | Grace Hopper portrait; source comment in Matplotlib points to Wikimedia Commons File:Grace Hopper.jpg; resized/cropped only | Public-domain U.S. Navy photograph |
| `gradient_circles.png` | Deterministic synthetic scene with gradients, circles and fine sinusoidal detail | Created for this project; regenerate with `python3 data/generate_synthetic.py` |

`data/color/` contains the three bundled colour test scenes used in the separate
YCbCr experiment. The experiment stores no personal images. Images from
`my_images/` remain ignored by Git.
