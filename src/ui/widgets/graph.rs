use adw::prelude::WidgetExt;
use gtk::glib::{self};
use gtk::subclass::prelude::*;
use plotters::style::RGBColor;

use std::f64;

use crate::utils::settings::SETTINGS;

const MAX_DATA_POINTS: u32 = 600;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
        error::Error,
    };

    use adw::prelude::SnapshotExt;
    use adw::prelude::WidgetExt;
    use gtk::{
        glib,
        subclass::{
            prelude::{ObjectImpl, ObjectSubclass, ObjectSubclassExt},
            widget::WidgetImpl,
        },
    };
    use plotters::{
        prelude::*,
        series::AreaSeries,
        style::{Color, RGBColor},
    };
    use plotters_cairo::CairoBackend;

    use crate::utils::settings::SETTINGS;

    use super::MAX_DATA_POINTS;

    #[derive(Debug)]
    pub struct ResGraph {
        pub data_points: RefCell<VecDeque<f64>>,
        pub max_y: Cell<Option<f64>>,
        pub graph_color: Cell<RGBColor>,
    }

    impl Default for ResGraph {
        fn default() -> Self {
            let mut empty_deque = VecDeque::with_capacity(MAX_DATA_POINTS as usize);
            for _ in 0..MAX_DATA_POINTS {
                empty_deque.push_back(0.0);
            }

            Self {
                data_points: RefCell::new(empty_deque),
                max_y: Cell::new(Some(1.0)),
                graph_color: Cell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResGraph {
        const NAME: &'static str = "ResGraph";
        type Type = super::ResGraph;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ResGraph {}

    impl WidgetImpl for ResGraph {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let width = self.obj().allocation().width() as u32;
            let height = self.obj().allocation().height() as u32;
            if width == 0 || height == 0 {
                return;
            }

            let bounds = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            let cr: gtk::cairo::Context = snapshot.append_cairo(&bounds);
            let backend = CairoBackend::new(&cr, (width, height)).unwrap();
            self.plot_graph(backend).unwrap();
        }
    }

    impl ResGraph {
        pub fn plot_graph<'a, DB>(&self, backend: DB) -> Result<(), Box<dyn Error + 'a>>
        where
            DB: DrawingBackend + 'a,
        {
            let data_points = self.data_points.borrow();
            let color = self.graph_color.get();

            let start_point =
                (MAX_DATA_POINTS.saturating_sub(SETTINGS.graph_data_points())) as usize;

            let root = backend.into_drawing_area();

            root.fill(&self.graph_color.get().mix(0.1))?;

            let y_max = self.max_y.get().unwrap_or_else(|| {
                let max = *data_points
                    .range(start_point..(MAX_DATA_POINTS as usize))
                    .max_by(|x, y| x.total_cmp(y))
                    .unwrap_or(&0.0);
                if max == 0.0 {
                    f64::EPSILON
                } else {
                    max
                }
            });

            let mut chart = ChartBuilder::on(&root).build_cartesian_2d(
                0f64..(SETTINGS.graph_data_points() as f64 - 1.0),
                0f64..y_max,
            )?;

            if SETTINGS.show_graph_grids() {
                chart
                    .configure_mesh()
                    .disable_axes()
                    .max_light_lines(0)
                    .bold_line_style(color.mix(0.4))
                    .draw()?;
            }

            chart.draw_series(
                AreaSeries::new(
                    (0..)
                        .zip(data_points.range(start_point..(MAX_DATA_POINTS as usize)))
                        .map(|(x, y)| (x as f64, *y)),
                    0.0,
                    color.mix(0.4),
                )
                .border_style(color),
            )?;

            root.present()?;
            Ok(())
        }
    }
}

glib::wrapper! {
    pub struct ResGraph(ObjectSubclass<imp::ResGraph>) @extends gtk::Widget;
}

impl Default for ResGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl ResGraph {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn set_graph_color(&self, r: u8, g: u8, b: u8) {
        let imp = self.imp();
        imp.graph_color.set(RGBColor(r, g, b));
        imp.obj().queue_draw();
    }

    pub fn set_locked_max_y(&self, y_max: Option<f64>) {
        let imp = self.imp();
        imp.max_y.set(y_max);
        imp.obj().queue_draw();
    }

    pub fn get_highest_value(&self) -> f64 {
        let imp = self.imp();

        let start_point = (MAX_DATA_POINTS.saturating_sub(SETTINGS.graph_data_points())) as usize;

        *imp.data_points
            .borrow()
            .range(start_point..(MAX_DATA_POINTS as usize))
            .max_by(|x, y| x.total_cmp(y))
            .unwrap_or(&0.0)
    }

    pub fn push_data_point(&self, data: f64) {
        let imp = self.imp();
        let mut data_points = imp.data_points.borrow_mut();
        if data_points.len() >= MAX_DATA_POINTS as usize {
            data_points.pop_front();
        }
        data_points.push_back(data);
        imp.obj().queue_draw();
    }

    pub fn data_points(&self) -> Vec<f64> {
        self.imp().data_points.borrow().iter().copied().collect()
    }

    pub fn push_data_points(&self, data: &[f64]) {
        let imp = self.imp();
        let mut data_points = imp.data_points.borrow_mut();
        for data_point in data {
            if data_points.len() >= MAX_DATA_POINTS as usize {
                data_points.pop_front();
            }
            data_points.push_back(*data_point);
        }
        imp.obj().queue_draw();
    }

    pub fn clear_data_points(&self) {
        self.imp().data_points.borrow_mut().clear();
    }
}
