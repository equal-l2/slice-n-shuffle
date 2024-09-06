use image::{GenericImageView, Pixel};

pub(crate) enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub(crate) fn compute_border_abs_diff<T>(my_view: &T, their_view: &T, dir: Direction) -> u64
where
    T: GenericImageView,
    T::Pixel: Pixel<Subpixel = u8>,
{
    assert!(my_view.dimensions() == their_view.dimensions());

    let (width, height) = my_view.dimensions();

    let (my_border, their_border) = match dir {
        Direction::Up => (
            my_view.view(0, 0, width, 1),
            their_view.view(0, height - 1, width, 1),
        ),
        Direction::Down => (
            my_view.view(0, height - 1, width, 1),
            their_view.view(0, 0, width, 1),
        ),
        Direction::Left => (
            my_view.view(0, 0, 1, height),
            their_view.view(width - 1, 0, 1, height),
        ),
        Direction::Right => (
            my_view.view(width - 1, 0, 1, height),
            their_view.view(0, 0, 1, height),
        ),
    };

    assert!(my_border.dimensions() == their_border.dimensions());

    std::iter::zip(my_border.pixels(), their_border.pixels())
        .map(|(a, b)| {
            let my_pixel = a.2.channels();
            let their_pixel = b.2.channels();
            std::iter::zip(my_pixel.iter(), their_pixel.iter())
                .map(|(a, b)| (u64::from(a.abs_diff(*b))))
                .sum::<u64>()
        })
        .sum::<u64>()
}
