use gtk::glib;
use gtk::subclass::prelude::*;
use gtk::traits::WidgetExt;
use plotters::style::RGBColor;

use std::f64;

mod imp {
    use std::{cell::RefCell, collections::VecDeque, error::Error};

    use gtk::{
        glib,
        subclass::{
            prelude::{ObjectImpl, ObjectSubclass, ObjectSubclassExt},
            widget::WidgetImpl,
        },
        traits::{SnapshotExt, WidgetExt},
    };
    use plotters::{
        prelude::*,
        series::AreaSeries,
        style::{Color, RGBColor, TRANSPARENT},
    };
    use plotters_cairo::CairoBackend;

    #[derive(Debug, Default)]
    pub struct ResGraph {
        pub data_points: RefCell<VecDeque<f64>>,
        pub data_points_max_amount: RefCell<usize>,
        pub graph_color: RefCell<RGBColor>,
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
            let cr = snapshot.append_cairo(&bounds);
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
            let data_points_max_amount = self.data_points_max_amount.borrow();
            let color = self.graph_color.borrow();

            let root = backend.into_drawing_area();

            root.fill(&TRANSPARENT)?;

            // in case we don't have enough data points for the whole graph
            // (because the program hasn't been running long enough e.g.),
            // fill it from the front with zeros until we have just enough
            // "space" for the actual data points
            let mut filled_data_points = vec![0.0; *data_points_max_amount - data_points.len()];
            for i in data_points.iter() {
                filled_data_points.push(*i);
            }

            let mut chart = ChartBuilder::on(&root)
                .build_cartesian_2d(0f64..(*data_points_max_amount as f64 - 1.0), 0f64..1.0)?;

            chart.draw_series(
                AreaSeries::new(
                    (0..)
                        .zip(filled_data_points.iter())
                        .map(|(x, y)| (x as f64, *y)),
                    0.0,
                    color.mix(0.4),
                )
                .border_style(*color),
            )?;

            root.present()?;
            Ok(())
        }
    }
}

glib::wrapper! {
    pub struct ResGraph(ObjectSubclass<imp::ResGraph>) @extends gtk::Widget;
}

impl ResGraph {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn set_data_points_max_amount(&self, max_amount: usize) {
        let imp = self.imp();
        *imp.data_points_max_amount.borrow_mut() = max_amount;
        imp.obj().queue_draw();
    }

    pub fn set_graph_color(&self, r: u8, g: u8, b: u8) {
        let imp = self.imp();
        *imp.graph_color.borrow_mut() = RGBColor(r, g, b);
        imp.obj().queue_draw();
    }

    pub fn push_data_point(&self, data: f64) {
        let imp = self.imp();
        let mut data_points = imp.data_points.borrow_mut();
        if data_points.len() >= *imp.data_points_max_amount.borrow() {
            data_points.pop_front();
        }
        data_points.push_back(data);
        imp.obj().queue_draw();
    }
}
